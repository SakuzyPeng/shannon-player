//! 解码线程 → 输出回调的无锁 SPSC 环形缓冲。
//!
//! 实时纪律（架构约束不变量第 3 条）：消费端只碰原子与自己的下标，
//! **不加锁、不分配、不做 I/O、不格式化日志**。因此这里手写而不是拿一个通用队列——
//! 通用队列多半按元素粒度操作，而输出回调要的是「一次拷走 N 帧」的批量语义。
//!
//! ## flush 由消费端执行
//!
//! seek 与切歌要丢弃在途 PCM。看似生产端把读下标推到写下标最省事，但那是错的：
//! 消费端可能正拷着 `[R, R+n)`，随后提交 `read = R+n`，把生产端的重置覆盖掉——
//! 结果是旧音频继续发声，正是 seek 最不能出的毛病。
//!
//! 所以约定：**读下标永远只由消费端写**。生产端发 flush 请求（自增计数），
//! 消费端在回调开头看到请求就丢弃全部未消费数据并回执。生产端只等回执，
//! **超时也绝不代写读下标**：超时只能说明回调没有及时响应，不能证明它已停止。
//! 若一个被调度器挂起的回调稍后恢复，生产端越权重置读下标会让两端同时访问同一段
//! `UnsafeCell` 缓冲，直接破坏本模块 `unsafe impl Sync` 的安全前提。
//! 暂停不走这条路——暂停时回调仍在向设备写零帧（见架构约束对暂停的定义），回执照常到达。
//!
//! ## 曲目边界打点
//!
//! gapless 意味着环形缓冲里会**同时躺着两首歌的 PCM**，中间没有任何分隔。于是有两件
//! 事只能在这里做：位置计数要在新曲的第一个样本处归零，「切歌」这个事实要在**消费端
//! 越过那一点时**才成立——解码可以领先播放一秒半，按解码时机判定会让界面提前切换。
//!
//! 打点因此记的是绝对样本下标，由消费端在读取时结算。生产端只从 [`take_crossed`] 得知
//! 「又越过了几个边界」，事件由控制线程发出：回调自己不发事件（实时纪律第 3 条）。
//!
//! ## 截断：让还没发声的 next 失效
//!
//! 用户在一首歌的最后一秒改了队列，此时下一首的开头已经写进缓冲。**输出端绝不能放出
//! 过期队列的下一首**，否则改队列对听感无效。丢弃它同样不能由生产端直接回退写下标——
//! 消费端可能正拷着那一段，理由与 flush 那节完全相同。
//!
//! 所以再加一条同样形状的协议：生产端发布一个消费上限 `limit` 并请求回执，消费端看到
//! 请求就回执，此后**读取一律不越过该上限**。生产端等到回执后再看读下标：
//!
//! - `read <= limit` —— 谁都没越过去，可以安全地把写下标回退到 `limit`，重写这一段；
//! - `read > limit` —— 已经发声了，那是既成事实，只能承认（见实现计划「队列归属与切歌
//!   交接」：已越过边界的切歌不得回退，也不得丢弃对应事件）。
//!
//! 作废的打点**不回收**：回退 `mark_write` 会与消费端正在读的槽位打架。它就留在原地，
//! 与新打点落在同一个绝对下标上，于是两者会被同时越过；哪一条该发事件由生产端自己记
//! （见 `engine.rs` 的 `marks`）。代价只是打点槽位的消耗，上限见 [`MARK_CAPACITY`]。

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 边界打点的槽位数。
///
/// 真正在途的边界通常只有一个（当前曲的末尾），极短曲目连续交接时会有两三个。留到 16
/// 是给「在最后一秒里反复改队列」的情况：每改一次会留下一条作废打点且不回收。用满了
/// 就退回非 gapless 路径（换曲之间有停顿），而不是丢弃打点——丢一条打点等于让位置计数
/// 与事件永久错位。
const MARK_CAPACITY: usize = 16;

/// 无截断上限时的哨兵值。
const NO_LIMIT: usize = usize::MAX;

/// 一次曲目边界。
#[derive(Clone, Copy, Default)]
struct Mark {
    /// 新曲第一个样本的绝对下标。
    boundary: usize,
    /// 越过之后位置计数从这里重新起算（帧）。整曲从头播就是 0。
    position_base: u64,
}

/// 消费端越过边界的结算结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crossing {
    /// 新曲的位置基准（帧）。
    pub position_base: u64,
    /// 越过边界之后本次又消费了多少帧。位置 = `position_base + frames_after`。
    pub frames_after: u64,
}

/// 一次读取的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReadOutcome {
    /// 实际读到的样本数；不足部分由调用方补零。
    pub samples: usize,
    /// 本次读取是否越过了曲目边界。跨过多个边界时给最后一个——中间那些曲目连一个
    /// 回调都没占满，位置只该落在真正在发声的那首上。
    pub crossed: Option<Crossing>,
}

struct Shared {
    /// 容量为 2 的幂，下标经 `& mask` 回绕。
    buf: UnsafeCell<Box<[f32]>>,
    mask: usize,
    /// 单调递增的样本计数，不回绕（usize 溢出需要连续播放数万年）。
    write: AtomicUsize,
    read: AtomicUsize,
    /// flush 请求 / 回执计数。相等表示无待处理请求。
    flush_req: AtomicUsize,
    flush_ack: AtomicUsize,
    /// 消费端不得越过的绝对样本下标（[`NO_LIMIT`] = 不限）。
    limit: AtomicUsize,
    /// limit 请求 / 回执计数。生产端据此确认上限已经生效。
    limit_req: AtomicUsize,
    limit_ack: AtomicUsize,
    /// 边界打点环。生产端只写 `[mark_crossed, mark_write)` 之外的槽位，
    /// 消费端只读该区间内的槽位——与数据缓冲同一套所有权切分。
    marks: UnsafeCell<[Mark; MARK_CAPACITY]>,
    mark_write: AtomicUsize,
    mark_crossed: AtomicUsize,
    channels: usize,
}

// 安全性：`buf` 的并发访问被 write / read 两个下标切分成互不相交的区间——
// 生产端只写 `[write, read + capacity)`，消费端只读 `[read, write)`，
// 且下标以 Acquire / Release 配对发布，因此不存在数据竞争。
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

/// 生产端（解码线程持有）。
pub struct RingProducer {
    shared: Arc<Shared>,
    /// 已经上报给控制线程的越界数。只有生产端读写，不必是原子量。
    reported_crossings: usize,
}

/// 消费端（输出回调持有）。
pub struct RingConsumer {
    shared: Arc<Shared>,
}

/// 建一对收发端。`capacity_frames` 会向上取到 2 的幂。
pub fn ring(capacity_frames: usize, channels: usize) -> (RingProducer, RingConsumer) {
    assert!(channels > 0, "声道数必须大于 0");
    let cap = (capacity_frames * channels).next_power_of_two();
    let shared = Arc::new(Shared {
        buf: UnsafeCell::new(vec![0.0; cap].into_boxed_slice()),
        mask: cap - 1,
        write: AtomicUsize::new(0),
        read: AtomicUsize::new(0),
        flush_req: AtomicUsize::new(0),
        flush_ack: AtomicUsize::new(0),
        limit: AtomicUsize::new(NO_LIMIT),
        limit_req: AtomicUsize::new(0),
        limit_ack: AtomicUsize::new(0),
        marks: UnsafeCell::new([Mark::default(); MARK_CAPACITY]),
        mark_write: AtomicUsize::new(0),
        mark_crossed: AtomicUsize::new(0),
        channels,
    });
    (
        RingProducer {
            shared: shared.clone(),
            reported_crossings: 0,
        },
        RingConsumer { shared },
    )
}

impl Shared {
    fn capacity(&self) -> usize {
        self.mask + 1
    }
}

impl RingProducer {
    /// 可写入的样本数。
    pub fn writable(&self) -> usize {
        let w = self.shared.write.load(Ordering::Relaxed);
        let r = self.shared.read.load(Ordering::Acquire);
        self.shared.capacity() - (w - r)
    }

    /// 已排队的帧数。用于喂料节流与 `bufferedSec` 换算。
    pub fn queued_frames(&self) -> usize {
        let w = self.shared.write.load(Ordering::Relaxed);
        let r = self.shared.read.load(Ordering::Acquire);
        (w - r) / self.shared.channels
    }

    pub fn capacity_frames(&self) -> usize {
        self.shared.capacity() / self.shared.channels
    }

    /// 写入尽可能多的样本，返回实际写入数（可能少于 `src.len()`）。
    pub fn write(&mut self, src: &[f32]) -> usize {
        let n = src.len().min(self.writable());
        if n == 0 {
            return 0;
        }
        let w = self.shared.write.load(Ordering::Relaxed);
        let buf = unsafe { &mut *self.shared.buf.get() };
        let start = w & self.shared.mask;
        let first = n.min(self.shared.capacity() - start);
        buf[start..start + first].copy_from_slice(&src[..first]);
        if first < n {
            buf[..n - first].copy_from_slice(&src[first..n]);
        }
        // Release：确保上面的写入对消费端的 Acquire 可见。
        self.shared.write.store(w + n, Ordering::Release);
        n
    }

    /// 请求丢弃全部未消费数据，并等待消费端回执。
    ///
    /// 返回是否等到了回执。超时只返回 `false`，不会修改消费端拥有的读下标；
    /// 调用方必须关闭输出流后再丢弃整对 ring，不能靠时间推断消费端已经消失。
    pub fn flush(&mut self, timeout: Duration) -> bool {
        // flush 丢掉的是全部在途数据，任何待生效的截断上限都随之失去意义；
        // 留着它只会把后续新写入的数据也挡在上限之外。
        self.shared.limit.store(NO_LIMIT, Ordering::Release);
        let req = self.shared.flush_req.load(Ordering::Relaxed) + 1;
        self.shared.flush_req.store(req, Ordering::Release);

        let acked = self.wait_ack(&self.shared.flush_ack, req, timeout);
        if acked {
            // 消费端已把打点一并作废（数据都没了，边界无从谈起）。生产端跟着对齐，
            // 否则下一次 take_crossed 会把这些从未发声的边界当成刚刚越过。
            self.reported_crossings = self.shared.mark_crossed.load(Ordering::Acquire);
        }
        acked
    }

    /// 当前写下标（绝对样本数）。打点与截断都以它为坐标。
    pub fn write_index(&self) -> usize {
        self.shared.write.load(Ordering::Relaxed)
    }

    /// 在当前写下标处打一个曲目边界：此处之后写入的样本属于新曲。
    ///
    /// 返回 `false` 表示打点槽位已满，调用方应当放弃这次无缝交接（退回「放完再装载」）。
    pub fn mark_boundary(&mut self, position_base: u64) -> bool {
        let write = self.shared.mark_write.load(Ordering::Relaxed);
        let crossed = self.shared.mark_crossed.load(Ordering::Acquire);
        if write - crossed >= MARK_CAPACITY {
            return false;
        }
        let boundary = self.shared.write.load(Ordering::Relaxed);
        // 安全性：`write % CAP` 落在 `[mark_crossed, mark_write)` 之外（上面刚确认还有余量），
        // 而消费端只读该区间内的槽位，因此这次写入不与它相交。
        unsafe {
            (*self.shared.marks.get())[write % MARK_CAPACITY] = Mark {
                boundary,
                position_base,
            };
        }
        self.shared.mark_write.store(write + 1, Ordering::Release);
        true
    }

    /// 自上次调用以来消费端又越过了几个边界。事件由控制线程按这个数发出。
    pub fn take_crossed(&mut self) -> usize {
        let crossed = self.shared.mark_crossed.load(Ordering::Acquire);
        let delta = crossed - self.reported_crossings;
        self.reported_crossings = crossed;
        delta
    }

    /// 把写下标回退到 `boundary`，丢弃其后尚未发声的样本。
    ///
    /// 返回是否成功。`false` 有两种原因，对调用方是同一件事——**那段音频已经是既成事实**：
    /// 消费端没在超时内回执（无法证明它不会继续读），或读下标已经越过了 `boundary`。
    ///
    /// 时序理由见模块头「截断」一节：回执之后的读取一定遵守上限，而回执之前的那次读取
    /// 已经把它的读下标发布出去了（同一条回调线程按序执行），所以回执后读到的
    /// `read <= boundary` 是可信的。
    pub fn truncate_after(&mut self, boundary: usize, timeout: Duration) -> bool {
        if boundary > self.shared.write.load(Ordering::Relaxed) {
            return false;
        }
        self.shared.limit.store(boundary, Ordering::Release);
        let req = self.shared.limit_req.load(Ordering::Relaxed) + 1;
        self.shared.limit_req.store(req, Ordering::Release);

        let ok = self.wait_ack(&self.shared.limit_ack, req, timeout)
            && self.shared.read.load(Ordering::Acquire) <= boundary;
        if ok {
            // 回退写下标：此刻消费端受上限约束，`[boundary, write)` 已确定无人访问。
            self.shared.write.store(boundary, Ordering::Release);
        }
        // 无论成败都要撤掉上限：成功时新数据就从 boundary 写起，失败时它更不该挡着后续播放。
        self.shared.limit.store(NO_LIMIT, Ordering::Release);
        ok
    }

    fn wait_ack(&self, ack: &AtomicUsize, req: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if ack.load(Ordering::Acquire) >= req {
                return true;
            }
            std::thread::yield_now();
        }
        false
    }
}

impl RingConsumer {
    /// 处理待办的 flush 与截断请求。**必须在回调开头无条件调用**，暂停时也不例外——
    /// 暂停期间回调不消费数据，若只在 [`read`](Self::read) 里处理，seek 就会一直等不到回执。
    ///
    /// 只有原子读写，满足实时纪律；幂等，重复调用无副作用。
    #[inline]
    pub fn poll_control(&self) {
        let req = self.shared.flush_req.load(Ordering::Acquire);
        if req != self.shared.flush_ack.load(Ordering::Relaxed) {
            let w = self.shared.write.load(Ordering::Acquire);
            self.shared.read.store(w, Ordering::Release);
            // 数据都丢了，指向它的边界也不再有意义。留着的话，生产端重新写满之后
            // 那些从未发声的边界会被当成刚刚越过，位置计数与切歌事件一起错位。
            let marks = self.shared.mark_write.load(Ordering::Acquire);
            self.shared.mark_crossed.store(marks, Ordering::Release);
            self.shared.flush_ack.store(req, Ordering::Release);
        }
        // 回执只表示「此后的读取遵守上限」。上限本身每次读取都重新取，不缓存。
        let req = self.shared.limit_req.load(Ordering::Acquire);
        if req != self.shared.limit_ack.load(Ordering::Relaxed) {
            self.shared.limit_ack.store(req, Ordering::Release);
        }
    }

    /// 可读样本数（已扣除截断上限）。
    pub fn readable(&self) -> usize {
        let limit = self.shared.limit.load(Ordering::Acquire);
        let w = self.shared.write.load(Ordering::Acquire).min(limit);
        let r = self.shared.read.load(Ordering::Relaxed);
        w.saturating_sub(r)
    }

    /// 读取到 `dst`；不足部分**由调用方补零**（欠载）。
    ///
    /// 越过曲目边界时一并结算位置基准——边界在缓冲里没有任何物理分隔，
    /// 只有这里知道读下标越过了它。
    #[inline]
    pub fn read(&mut self, dst: &mut [f32]) -> ReadOutcome {
        self.poll_control();
        let n = dst.len().min(self.readable());
        if n == 0 {
            return ReadOutcome::default();
        }
        let r = self.shared.read.load(Ordering::Relaxed);
        let buf = unsafe { &*self.shared.buf.get() };
        let start = r & self.shared.mask;
        let first = n.min(self.shared.capacity() - start);
        dst[..first].copy_from_slice(&buf[start..start + first]);
        if first < n {
            dst[first..n].copy_from_slice(&buf[..n - first]);
        }
        let end = r + n;
        self.shared.read.store(end, Ordering::Release);
        ReadOutcome {
            samples: n,
            crossed: self.settle_marks(end),
        }
    }

    /// 结算本次读取越过的边界。返回最后一个——被整个跨过去的曲目（短于一次回调）
    /// 此刻已经放完了，位置该落在真正还在发声的那首上。
    #[inline]
    fn settle_marks(&self, read_end: usize) -> Option<Crossing> {
        let mut index = self.shared.mark_crossed.load(Ordering::Relaxed);
        let published = self.shared.mark_write.load(Ordering::Acquire);
        let mut last: Option<Mark> = None;
        while index < published {
            // 安全性：`[mark_crossed, mark_write)` 内的槽位生产端不再触碰。
            let mark = unsafe { (*self.shared.marks.get())[index % MARK_CAPACITY] };
            // 边界处的样本尚未被消费时不算越过：读到边界为止 = 新曲一个样本都没出声。
            if read_end <= mark.boundary {
                break;
            }
            last = Some(mark);
            index += 1;
        }
        let mark = last?;
        self.shared.mark_crossed.store(index, Ordering::Release);
        Some(Crossing {
            position_base: mark.position_base,
            frames_after: ((read_end - mark.boundary) / self.shared.channels) as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只关心读了多少的用例用它，省得每处都写 `.samples`。
    fn read_samples(rx: &mut RingConsumer, dst: &mut [f32]) -> usize {
        rx.read(dst).samples
    }

    /// 在「回调持续运行」的前提下执行一段生产端操作。
    ///
    /// flush 与截断都要等消费端回执，而回执只能来自回调。暂停时回调同样照跑
    /// （只写零帧，见模块头），所以这个前提在真实链路里始终成立；这里只是把它显式化。
    /// 轮询线程**不消费数据**，读下标因此仍由用例自己掌控。
    fn with_running_callback<T>(rx: &RingConsumer, action: impl FnOnce() -> T) -> T {
        let stop = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    rx.poll_control();
                    std::thread::yield_now();
                }
            });
            let out = action();
            stop.store(true, Ordering::Relaxed);
            out
        })
    }

    #[test]
    fn write_then_read_roundtrip() {
        let (mut tx, mut rx) = ring(8, 2);
        assert_eq!(tx.write(&[1.0, 2.0, 3.0, 4.0]), 4);
        let mut out = [0.0; 4];
        assert_eq!(read_samples(&mut rx, &mut out), 4);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn write_stops_at_capacity() {
        let (mut tx, _rx) = ring(4, 2); // 容量 8 个样本
        let src = vec![1.0; 32];
        assert_eq!(tx.write(&src), 8);
        assert_eq!(tx.write(&src), 0);
    }

    #[test]
    fn wraps_around_without_losing_order() {
        let (mut tx, mut rx) = ring(4, 1); // 容量 4
        tx.write(&[1.0, 2.0, 3.0]);
        let mut out = [0.0; 3];
        read_samples(&mut rx, &mut out);
        // 此时下标已推进到 3，再写 3 个必然跨越回绕点。
        tx.write(&[4.0, 5.0, 6.0]);
        read_samples(&mut rx, &mut out);
        assert_eq!(out, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn partial_read_reports_actual_count() {
        let (mut tx, mut rx) = ring(8, 1);
        tx.write(&[1.0, 2.0]);
        let mut out = [-1.0; 5];
        assert_eq!(read_samples(&mut rx, &mut out), 2);
        // 剩余部分保持原样，补零是调用方的职责（欠载要能被统计到）。
        assert_eq!(out[2], -1.0);
    }

    #[test]
    fn timed_out_flush_waits_for_consumer_to_discard_audio() {
        let (mut tx, mut rx) = ring(16, 1);
        tx.write(&[1.0; 8]);
        // 超时不允许生产端越权改读下标；消费端稍后恢复时仍会先处理请求，
        // 因而旧数据一样不会漏出去，同时不破坏 SPSC 的所有权约束。
        assert!(!tx.flush(Duration::from_millis(1)));
        let mut out = [0.0; 8];
        assert_eq!(
            read_samples(&mut rx, &mut out),
            0,
            "消费端恢复后不得再读到旧数据"
        );

        // flush 之后仍可正常收发。
        tx.write(&[9.0; 4]);
        assert_eq!(read_samples(&mut rx, &mut out), 4);
        assert_eq!(out[0], 9.0);
    }

    #[test]
    fn crossing_a_boundary_restarts_the_position_count() {
        // 立体声：4 帧旧曲 + 新曲若干，一次读完。位置不该是「累加」而是「从新曲起算」。
        let (mut tx, mut rx) = ring(64, 2);
        tx.write(&[1.0; 8]); // 旧曲 4 帧
        assert!(tx.mark_boundary(0));
        tx.write(&[2.0; 6]); // 新曲 3 帧

        let mut out = [0.0; 14];
        let outcome = rx.read(&mut out);
        assert_eq!(outcome.samples, 14);
        let crossing = outcome.crossed.expect("读过了边界就必须结算");
        assert_eq!(crossing.position_base, 0);
        assert_eq!(crossing.frames_after, 3, "只算边界之后的帧");
    }

    #[test]
    fn stopping_exactly_at_the_boundary_is_not_a_crossing() {
        // 差一个样本就是「新曲还没出声」。这一格决定了界面何时切歌，不能含糊。
        let (mut tx, mut rx) = ring(64, 1);
        tx.write(&[1.0; 4]);
        assert!(tx.mark_boundary(0));
        tx.write(&[2.0; 4]);

        let mut out = [0.0; 4];
        assert!(rx.read(&mut out).crossed.is_none(), "读到边界为止不算越过");
        let mut one = [0.0; 1];
        assert!(rx.read(&mut one).crossed.is_some(), "再读一个样本才算");
    }

    #[test]
    fn a_track_shorter_than_one_callback_settles_on_the_last_boundary() {
        // 极短曲目（几毫秒的间奏）会在一次回调里被整个跨过去。位置必须落在**还在发声**
        // 的那首上；取第一个边界会让进度停在一首已经放完的曲子上。
        let (mut tx, mut rx) = ring(64, 1);
        tx.write(&[1.0; 4]);
        assert!(tx.mark_boundary(0));
        tx.write(&[2.0; 2]); // 只有 2 帧的一首
        assert!(tx.mark_boundary(0));
        tx.write(&[3.0; 5]);

        let mut out = [0.0; 11];
        let crossing = rx.read(&mut out).crossed.expect("越过了两个边界");
        assert_eq!(crossing.frames_after, 5, "位置应当落在最后那首上");
        assert_eq!(tx.take_crossed(), 2, "两个边界都要上报，事件不能少发");
    }

    #[test]
    fn truncation_discards_audio_that_has_not_been_heard() {
        // 用户在最后一秒改了队列：已经写进缓冲的下一首必须消失，否则改队列对听感无效。
        let (mut tx, mut rx) = ring(64, 1);
        tx.write(&[1.0; 4]);
        let boundary = tx.write_index();
        assert!(tx.mark_boundary(0));
        tx.write(&[7.0; 4]); // 旧的下一首

        // 消费端还没碰到边界。
        assert!(with_running_callback(&rx, || tx
            .truncate_after(boundary, Duration::from_secs(2))));

        tx.write(&[9.0; 3]); // 新的下一首
        assert!(tx.mark_boundary(0));
        let mut out = [0.0; 16];
        let outcome = rx.read(&mut out);
        assert_eq!(outcome.samples, 7);
        assert!(
            !out[..7].contains(&7.0),
            "旧队列的下一首一个样本都不许出去：{:?}",
            &out[..7]
        );
    }

    #[test]
    fn truncation_fails_once_the_audio_has_already_been_consumed() {
        // 越过边界之后再改队列就晚了。此时必须如实回答「没能撤掉」，
        // 让上层按既成事实处理，而不是回退一段已经发声的音频。
        let (mut tx, mut rx) = ring(64, 1);
        tx.write(&[1.0; 4]);
        let boundary = tx.write_index();
        assert!(tx.mark_boundary(0));
        tx.write(&[7.0; 4]);

        let mut out = [0.0; 6];
        assert!(rx.read(&mut out).crossed.is_some());
        assert!(!tx.truncate_after(boundary, Duration::from_millis(50)));
        // 失败之后上限必须撤掉，否则剩下的音频再也放不出去。
        let mut rest = [0.0; 4];
        assert_eq!(read_samples(&mut rx, &mut rest), 2);
    }

    #[test]
    fn flush_also_voids_pending_boundaries() {
        // seek 丢掉全部在途 PCM，指向它的边界也随之作废。留着的话，缓冲重新填满后
        // 那个从未发声的边界会被当成刚刚越过，位置计数与切歌事件一起错位。
        let (mut tx, mut rx) = ring(64, 1);
        tx.write(&[1.0; 4]);
        assert!(tx.mark_boundary(0));
        tx.write(&[2.0; 4]);

        assert!(with_running_callback(&rx, || tx.flush(Duration::from_secs(2))));
        assert_eq!(tx.take_crossed(), 0, "作废的边界不是「越过」");

        tx.write(&[5.0; 6]);
        let mut out = [0.0; 6];
        assert!(rx.read(&mut out).crossed.is_none(), "重填后不该冒出旧边界");
    }

    #[test]
    fn boundary_slots_are_finite_and_say_so() {
        // 满了要如实返回 false，让上层退回「放完再装载」。悄悄丢一条打点等于
        // 让位置计数与事件永久错位，比一次可解释的停顿糟糕得多。
        let (mut tx, _rx) = ring(1024, 1);
        for _ in 0..MARK_CAPACITY {
            assert!(tx.mark_boundary(0));
            tx.write(&[0.5; 1]);
        }
        assert!(!tx.mark_boundary(0));
    }

    #[test]
    fn consumer_acknowledges_flush() {
        let (mut tx, mut rx) = ring(16, 1);
        tx.write(&[1.0; 8]);
        // 模拟回调：flush 请求发出后由消费端处理。
        let handle = std::thread::spawn(move || {
            let mut out = [0.0; 8];
            let mut spins = 0;
            // 回调持续运行（暂停时也照跑，只是写零帧）。
            while spins < 10_000 {
                if read_samples(&mut rx, &mut out) == 0 && rx.readable() == 0 {
                    spins += 1;
                }
                std::thread::yield_now();
            }
        });
        assert!(tx.flush(Duration::from_secs(2)), "回调在跑时必须等到回执");
        handle.join().unwrap();
    }
}
