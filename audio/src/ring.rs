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
//! 消费端在回调开头看到请求就丢弃全部未消费数据并回执。生产端等回执，
//! 等不到就说明输出流没在跑（Idle / 未启动），此时没有并发消费者，自行重置是安全的。
//! 暂停不走这条路——暂停时回调仍在向设备写零帧（见架构约束对暂停的定义），回执照常到达。

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        channels,
    });
    (RingProducer { shared: shared.clone() }, RingConsumer { shared })
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
    /// 返回是否等到了回执。等不到（`timeout` 到期）说明输出回调没在运行，
    /// 此时无并发消费者，函数会自行重置下标——同样达成目的，只是路径不同。
    pub fn flush(&mut self, timeout: Duration) -> bool {
        let req = self.shared.flush_req.load(Ordering::Relaxed) + 1;
        self.shared.flush_req.store(req, Ordering::Release);

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.shared.flush_ack.load(Ordering::Acquire) >= req {
                return true;
            }
            std::thread::yield_now();
        }

        // 回调没在跑：读下标此刻无人写，生产端直接对齐即可。
        let w = self.shared.write.load(Ordering::Relaxed);
        self.shared.read.store(w, Ordering::Release);
        self.shared.flush_ack.store(req, Ordering::Release);
        false
    }
}

impl RingConsumer {
    /// 处理待办的 flush 请求。**必须在回调开头无条件调用**，暂停时也不例外——
    /// 暂停期间回调不消费数据，若只在 [`read`](Self::read) 里处理，seek 就会一直等不到回执。
    ///
    /// 只有原子读写，满足实时纪律；幂等，重复调用无副作用。
    #[inline]
    pub fn poll_flush(&self) {
        let req = self.shared.flush_req.load(Ordering::Acquire);
        if req != self.shared.flush_ack.load(Ordering::Relaxed) {
            let w = self.shared.write.load(Ordering::Acquire);
            self.shared.read.store(w, Ordering::Release);
            self.shared.flush_ack.store(req, Ordering::Release);
        }
    }

    /// 可读样本数。
    pub fn readable(&self) -> usize {
        let w = self.shared.write.load(Ordering::Acquire);
        let r = self.shared.read.load(Ordering::Relaxed);
        w - r
    }

    /// 读取到 `dst`，返回实际读到的样本数；不足部分**由调用方补零**（欠载）。
    #[inline]
    pub fn read(&mut self, dst: &mut [f32]) -> usize {
        self.poll_flush();
        let n = dst.len().min(self.readable());
        if n == 0 {
            return 0;
        }
        let r = self.shared.read.load(Ordering::Relaxed);
        let buf = unsafe { &*self.shared.buf.get() };
        let start = r & self.shared.mask;
        let first = n.min(self.shared.capacity() - start);
        dst[..first].copy_from_slice(&buf[start..start + first]);
        if first < n {
            dst[first..n].copy_from_slice(&buf[..n - first]);
        }
        self.shared.read.store(r + n, Ordering::Release);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrip() {
        let (mut tx, mut rx) = ring(8, 2);
        assert_eq!(tx.write(&[1.0, 2.0, 3.0, 4.0]), 4);
        let mut out = [0.0; 4];
        assert_eq!(rx.read(&mut out), 4);
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
        rx.read(&mut out);
        // 此时下标已推进到 3，再写 3 个必然跨越回绕点。
        tx.write(&[4.0, 5.0, 6.0]);
        rx.read(&mut out);
        assert_eq!(out, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn partial_read_reports_actual_count() {
        let (mut tx, mut rx) = ring(8, 1);
        tx.write(&[1.0, 2.0]);
        let mut out = [-1.0; 5];
        assert_eq!(rx.read(&mut out), 2);
        // 剩余部分保持原样，补零是调用方的职责（欠载要能被统计到）。
        assert_eq!(out[2], -1.0);
    }

    #[test]
    fn flush_discards_pending_audio() {
        let (mut tx, mut rx) = ring(16, 1);
        tx.write(&[1.0; 8]);
        // 没有并发消费者，走超时自行重置那条路。
        assert!(!tx.flush(Duration::from_millis(1)));
        let mut out = [0.0; 8];
        assert_eq!(rx.read(&mut out), 0, "flush 后不得再读到旧数据");

        // flush 之后仍可正常收发。
        tx.write(&[9.0; 4]);
        assert_eq!(rx.read(&mut out), 4);
        assert_eq!(out[0], 9.0);
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
                if rx.read(&mut out) == 0 && rx.readable() == 0 {
                    spins += 1;
                }
                std::thread::yield_now();
            }
        });
        assert!(tx.flush(Duration::from_secs(2)), "回调在跑时必须等到回执");
        handle.join().unwrap();
    }
}
