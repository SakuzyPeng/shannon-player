//! 实时纪律的机器验证：输出回调内不发生动态分配（验收条件第 5 条的前半句）。
//!
//! 被测对象是 `render_output_callback` —— **生产代码里那一整段回调体**，不是它的复制品，
//! 也不只是 `fill_from_ring` 那个填充核心。CPAL 的闭包如今只负责把设备缓冲与时间戳递进去，
//! 因此这里跑的就是真机上跑的那段（见 `docs/AUDIO_BACKEND_IMPLEMENTATION_PLAN.md`
//! 「实时纪律的机器验证」）。
//!
//! ## 为什么是独立的测试文件
//!
//! `#[global_allocator]` 是每个二进制一份。Cargo 给每个 `tests/*.rs` 单独生成二进制，
//! 所以这里装的计数分配器只作用于本文件，既不波及其余用例，也不会进产物。
//!
//! ## 为什么是确定性驱动而不是压力循环
//!
//! 「压力测试」容易被写成「按真实节拍跑几秒，指望撞上各个分支」。那样一来覆盖取决于机器
//! 当时的负载，CI 上必然抖，而且失败时说不清到底走没走到那条分支。这里改为**逐次调用、
//! 逐次断言**：每个分支由测试显式摆出前置状态再触发，回调调用了几次、走的哪条路都是确定的。
//!
//! ## 这里证明不了什么
//!
//! 计数器只回答「有没有动态分配」。拿锁、文件 I/O 与系统调用都可能零分配，**不阻塞**那半句
//! 靠的是另一份证据：武装区间内可触碰的对象只有预分配切片、`OutputShared` 的原子量与无锁
//! 的 `RingConsumer`。两份证据都齐才满足验收条件第 5 条。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::thread::LocalKey;
use std::time::Duration;

use shannon_audio::output::{render_output_callback, CallbackState, OutputShared};
use shannon_audio::ring::{ring, RingConsumer, RingProducer};

/* ============================================================
计数分配器
============================================================ */

struct CountingAllocator;

// 开关与三个计数器都是**线程局部**的：测试二进制里的用例默认并行跑，用全局计数会把别的
// 线程的分配算到被测区间头上。
thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static DEALLOCS: Cell<u64> = const { Cell::new(0) };
    static REALLOCS: Cell<u64> = const { Cell::new(0) };
}

/// 计数。用 `try_with`：线程刚起或正在析构 TLS 时访问会失败，此时按「未武装」处理——
/// 那两个时刻都不在被测区间内，而在分配器里 panic 会把进程直接带走。
fn bump(counter: &'static LocalKey<Cell<u64>>) {
    if ARMED.try_with(Cell::get).unwrap_or(false) {
        let _ = counter.try_with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump(&ALLOCS);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        bump(&DEALLOCS);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump(&REALLOCS);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump(&ALLOCS);
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    alloc: u64,
    dealloc: u64,
    realloc: u64,
}

impl Counts {
    fn is_zero(&self) -> bool {
        *self == Counts::default()
    }
}

/// 武装计数器跑一段，返回它的结果与这段里的分配次数。
///
/// 两处刻意为之：① 先摸一遍 TLS 再清零——`thread_local!` 首次访问要走一次初始化，
/// 把它算进被测区间会得到一个与被测代码无关的计数；② 武装期间不做断言、不打日志、
/// 不与其它线程通信，夹具自己的开销一律留到解除武装之后。
fn armed<R>(body: impl FnOnce() -> R) -> (R, Counts) {
    ARMED.with(|a| a.set(false));
    ALLOCS.with(|c| c.set(0));
    DEALLOCS.with(|c| c.set(0));
    REALLOCS.with(|c| c.set(0));

    ARMED.with(|a| a.set(true));
    let out = body();
    ARMED.with(|a| a.set(false));

    let counts = Counts {
        alloc: ALLOCS.with(Cell::get),
        dealloc: DEALLOCS.with(Cell::get),
        realloc: REALLOCS.with(Cell::get),
    };
    (out, counts)
}

/// 自检：夹具真的数得到分配，否则「全为 0」只说明计数器坏了。
#[test]
fn the_counter_actually_counts() {
    let (_, counts) = armed(|| {
        let v: Vec<u8> = Vec::with_capacity(1024);
        std::hint::black_box(&v);
    });
    assert!(counts.alloc >= 1, "武装区间内的一次 Vec 分配必须被数到");
    assert!(
        counts.dealloc >= 1,
        "Vec 在区间内析构，释放同样要被数到：只数 alloc 会漏掉「借了又还」的路径"
    );

    let (_, idle) = armed(|| std::hint::black_box(1u32));
    assert!(idle.is_zero(), "什么都不做时不该有分配");
}

/* ============================================================
回调夹具
============================================================ */

const CHANNELS: usize = 2;
const RING_FRAMES: usize = 4096;
/// scratch 比输出缓冲小，逼 `render_output_callback` 走多次分块——真机上回调请求超过
/// scratch 时走的就是这条路，而它正是最容易被顺手加一句 `Vec` 的地方。
const SCRATCH_FRAMES: usize = 32;
const OUT_FRAMES: usize = 128;

/// 一套备好的回调现场。所有缓冲都在武装之前分配完。
struct Callback {
    out: Vec<f32>,
    state: CallbackState,
}

impl Callback {
    fn new() -> Self {
        Self::with_out_frames(OUT_FRAMES)
    }

    fn with_out_frames(frames: usize) -> Self {
        Self {
            out: vec![0.0; frames * CHANNELS],
            // 采样率取 1000：`ramp_step_for` 于是给出 1/15 以外的整齐值不重要，
            // 重要的是斜坡够快，几帧内收敛，样本值可预期——斜坡本身另有用例。
            state: CallbackState::new(CHANNELS, 1_000, SCRATCH_FRAMES),
        }
    }

    /// 跑一次回调，返回这次的分配计数。
    fn run(&mut self, consumer: &mut RingConsumer, shared: &OutputShared) -> Counts {
        let (_, counts) = armed(|| {
            render_output_callback(
                &mut self.out,
                &mut self.state,
                consumer,
                shared,
                // 真机上这里是 CPAL 的时间戳换算；换算本身是纯算术，闭包按泛型传入、不装箱。
                || 480,
            );
        });
        counts
    }
}

fn full_ring() -> (RingProducer, RingConsumer, OutputShared) {
    let (producer, consumer) = ring(RING_FRAMES, CHANNELS);
    let shared = OutputShared::default();
    shared.set_paused(false);
    shared.set_gain(1.0);
    (producer, consumer, shared)
}

/// 往 ring 里灌 `frames` 帧恒定值。
fn feed(producer: &mut RingProducer, frames: usize, value: f32) -> usize {
    let block = vec![value; frames * CHANNELS];
    producer.write(&block)
}

/* ============================================================
压力项：每一项都断言「零分配」+「行为仍然正确」
只断言零分配是不够的——一个什么都不做的回调也满足它。
============================================================ */

#[test]
fn steady_playback_allocates_nothing() {
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();

    for round in 0..8 {
        feed(&mut producer, OUT_FRAMES, 0.5);
        let counts = cb.run(&mut consumer, &shared);
        assert!(
            counts.is_zero(),
            "第 {round} 次常规回调发生了分配：{counts:?}"
        );
    }

    assert_eq!(
        shared.position_frames(),
        (OUT_FRAMES * 8) as u64,
        "位置要按消费帧数推进；只证明没分配、却没在搬数据是假通过"
    );
    assert_eq!(shared.underruns(), 0, "供给充足时不该有欠载");
    assert_eq!(
        shared.output_delay_frames(),
        480,
        "设备延迟由回调体写入，提取之后这条不能丢"
    );
    assert!(
        cb.out.iter().all(|s| (*s - 0.5).abs() < 1e-6),
        "增益到位后输出应等于源样本"
    );
}

#[test]
fn underrun_path_allocates_nothing() {
    let (_producer, mut consumer, shared) = full_ring();
    // 一次回调只占一块 scratch —— 真机上的常态（scratch 是 8192 帧，设备缓冲通常几百帧）。
    let mut cb = Callback::with_out_frames(SCRATCH_FRAMES / 2);

    // ring 全空、正在播放、不在重缓冲也没播完 —— 这才是一次真实欠载。
    let counts = cb.run(&mut consumer, &shared);

    assert!(counts.is_zero(), "欠载补零路径发生了分配：{counts:?}");
    assert_eq!(shared.underruns(), 1, "取不到数据要计一次欠载");
    assert!(cb.out.iter().all(|s| *s == 0.0), "取不到数据时补零");
}

/// 记录一处**现有行为**：欠载数按分块计，不按回调计。
///
/// 设备缓冲超过 scratch（8192 帧）时一次回调会被拆成多块，每块各记一次欠载，于是同一次
/// 掉音在不同设备上得到不同的数字，调小 scratch 也会让这个指标凭空变大——而它要回答的是
/// 「实时性够不够」。真机上很少触发（8192 帧在 48 kHz 上是 170 ms），所以这里先把行为钉住
/// 而不是顺手改掉：改动的是一个对外暴露的指标口径，该由维护者定。
#[test]
fn a_starved_callback_currently_counts_one_underrun_per_chunk() {
    let (_producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::with_out_frames(SCRATCH_FRAMES * 4);

    let counts = cb.run(&mut consumer, &shared);

    assert!(counts.is_zero(), "分块欠载路径发生了分配：{counts:?}");
    assert_eq!(
        shared.underruns(),
        4,
        "当前实现按 scratch 分块计数；改成「一次回调至多一次欠载」时这里要一起改"
    );
}

#[test]
fn paused_paths_allocate_nothing() {
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();
    feed(&mut producer, OUT_FRAMES * 4, 0.5);

    // 先出声，把增益抬起来。
    cb.run(&mut consumer, &shared);
    let played = shared.position_frames();

    // ① 刚暂停：增益还没落到零，走的是「照常消费 + 斜坡下行」的混合路径。
    shared.set_paused(true);
    let ramping = cb.run(&mut consumer, &shared);
    assert!(ramping.is_zero(), "暂停斜坡路径发生了分配：{ramping:?}");
    assert!(
        shared.position_frames() > played,
        "斜坡未收敛时仍在消费数据，位置要继续走"
    );

    // ② 已静音且暂停：提前返回的纯零帧路径，维持设备时钟但不推进位置。
    let silent = shared.position_frames();
    let quiet = cb.run(&mut consumer, &shared);
    assert!(quiet.is_zero(), "暂停零帧路径发生了分配：{quiet:?}");
    assert_eq!(
        shared.position_frames(),
        silent,
        "静音暂停不消费数据，位置不该推进"
    );
    assert!(cb.out.iter().all(|s| *s == 0.0), "暂停期间只写零帧");
    assert_eq!(shared.underruns(), 0, "暂停不是欠载");
}

#[test]
fn crossing_a_track_boundary_allocates_nothing() {
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();

    // 上一首剩半个回调的量，随后打点、写下一首。
    let head_frames = OUT_FRAMES / 2;
    feed(&mut producer, head_frames, 0.25);
    assert!(producer.mark_boundary(1_000), "打点槽位应当有余量");
    feed(&mut producer, OUT_FRAMES, 0.75);

    let counts = cb.run(&mut consumer, &shared);

    assert!(counts.is_zero(), "越过曲目边界发生了分配：{counts:?}");
    assert_eq!(
        shared.position_frames(),
        1_000 + (OUT_FRAMES - head_frames) as u64,
        "越界后位置是**改写**成新曲基准加上越界后的帧数，不是继续累加"
    );
    assert_eq!(
        shared.total_frames(),
        OUT_FRAMES as u64,
        "累计量跨曲目单调递增，边界处照加不误"
    );
    assert_eq!(producer.take_crossed(), 1, "生产端要能收到这次越界");
}

#[test]
fn flushing_for_a_seek_allocates_nothing() {
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();
    feed(&mut producer, OUT_FRAMES * 4, 0.5);
    cb.run(&mut consumer, &shared);

    // flush 要等消费端回执，只能由另一条线程发起；回调仍由本线程逐次驱动，
    // 于是「跑了几次回调」是确定的，不靠 sleep 赌时序。
    let flusher = std::thread::spawn(move || {
        let acked = producer.flush(Duration::from_secs(5));
        (producer, acked)
    });

    let mut rounds = 0;
    let acked = loop {
        let counts = cb.run(&mut consumer, &shared);
        assert!(counts.is_zero(), "处理 flush 的回调发生了分配：{counts:?}");
        rounds += 1;
        if flusher.is_finished() {
            break flusher.join().expect("flush 线程 panic").1;
        }
        assert!(rounds < 10_000, "flush 迟迟等不到回执");
    };

    assert!(acked, "回调持续在跑时 flush 必须能等到回执");
    assert_eq!(consumer.readable(), 0, "flush 之后在途数据要被丢干净");
}

#[test]
fn truncating_a_staged_track_allocates_nothing() {
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();
    // 暂停：这条路径的前提正是「下一首已经写进缓冲但还没发声」，消费端不能越过 boundary。
    shared.set_paused(true);

    feed(&mut producer, OUT_FRAMES, 0.25);
    let boundary = producer.write_index();
    assert!(producer.mark_boundary(0), "打点槽位应当有余量");
    feed(&mut producer, OUT_FRAMES, 0.75);

    let truncator = std::thread::spawn(move || {
        let ok = producer.truncate_after(boundary, Duration::from_secs(5));
        (producer, ok)
    });

    let mut rounds = 0;
    let (producer, ok) = loop {
        let counts = cb.run(&mut consumer, &shared);
        assert!(counts.is_zero(), "处理截断的回调发生了分配：{counts:?}");
        rounds += 1;
        if truncator.is_finished() {
            break truncator.join().expect("截断线程 panic");
        }
        assert!(rounds < 10_000, "截断迟迟等不到回执");
    };

    assert!(ok, "消费端没越过边界时截断必须成功");
    assert_eq!(
        producer.write_index(),
        boundary,
        "截断成功后写下标要回退到边界"
    );
    assert_eq!(
        consumer.readable(),
        boundary,
        "边界之后那段没发声的音频要被撤掉"
    );
}

#[test]
fn switching_endpoints_allocates_nothing() {
    // 换端点在回调这一侧**不走 flush**：旧 ring 正常回调 → 暂停并标记重缓冲后的尾回调 →
    // 控制线程停掉旧流 → 新 ring 上跑首次回调。这里逐段验证会运行的回调体都不分配。
    // 协商、关流、重开与位置接续不在实时线程，由 `tests/playback.rs` 的设备切换用例负责。
    let (mut producer, mut consumer, shared) = full_ring();
    let mut cb = Callback::new();
    feed(&mut producer, OUT_FRAMES * 5, 0.5);

    // 多跑几轮，让人工设置的 480 帧设备延迟之后确实已有样本发声；否则保存位置恒为 0，
    // 「接着走」的断言只是碰巧通过。
    for round in 0..4 {
        let running = cb.run(&mut consumer, &shared);
        assert!(
            running.is_zero(),
            "换端点前第 {round} 次常规回调发生了分配：{running:?}"
        );
    }
    let saved_position = shared.played_frames();
    assert!(saved_position > 0, "测试前置条件：旧端点必须已有样本发声");

    shared.set_paused(true);
    shared.set_rebuffering(true);
    let tail = cb.run(&mut consumer, &shared);
    assert!(tail.is_zero(), "拆流前的尾回调发生了分配：{tail:?}");

    drop(consumer);
    drop(producer);

    // 新端点只重建 ring，OutputShared 与真实 switch_device 一样原样复用。open 会立即触发
    // 首次回调，那时仍是 paused + rebuffering，ring 也尚未预填，必须安静且不记欠载。
    let (mut next_producer, mut next_consumer) = ring(RING_FRAMES, CHANNELS);
    shared.reset_position(saved_position);
    shared.set_gain(1.0);
    shared.set_source_drained(false);
    shared.set_rebuffering(true);
    shared.set_paused(true);
    shared.reset_callback_timing();
    let mut next_cb = Callback::new();
    let opening = next_cb.run(&mut next_consumer, &shared);

    assert!(
        opening.is_zero(),
        "新端点打开时的静音回调发生了分配：{opening:?}"
    );
    assert_eq!(
        shared.position_frames(),
        saved_position,
        "新流尚未预填且仍暂停时，首次回调不得推进位置"
    );
    assert_eq!(shared.underruns(), 0, "重缓冲中的空读不是欠载");

    // 预填完成后，SetDevice 的命令处理会按原传输意图解除暂停；下一次才是首个有声回调。
    feed(&mut next_producer, OUT_FRAMES, 0.5);
    shared.set_rebuffering(false);
    shared.set_paused(false);
    let first = next_cb.run(&mut next_consumer, &shared);

    assert!(first.is_zero(), "新端点的首个有声回调发生了分配：{first:?}");
    assert_eq!(
        shared.underruns(),
        0,
        "新流首个有声回调有数据可读，不该记欠载"
    );
    assert_eq!(
        shared.position_frames(),
        saved_position + OUT_FRAMES as u64,
        "换端点后位置要从旧端点已发声的位置接着走"
    );
}

#[test]
fn every_supported_sample_conversion_allocates_nothing() {
    // `build_stream` 会为这五种格式分别单态化回调。一种 T 通过不能替其它 T 作证，
    // 所以每条生产分支都必须实际执行一次。
    macro_rules! check_format {
        ($ty:ty) => {{
            let (mut producer, mut consumer, shared) = full_ring();
            feed(&mut producer, OUT_FRAMES, 1.0);
            let mut out = vec![<$ty>::default(); OUT_FRAMES * CHANNELS];
            let mut state = CallbackState::new(CHANNELS, 1_000, SCRATCH_FRAMES);

            let (_, counts) = armed(|| {
                render_output_callback(&mut out, &mut state, &mut consumer, &shared, || 0);
            });

            assert!(
                counts.is_zero(),
                "{} 转换路径发生了分配：{counts:?}",
                stringify!($ty)
            );
            assert_eq!(
                shared.position_frames(),
                OUT_FRAMES as u64,
                "{} 转换路径仍须真正消费完整个设备缓冲",
                stringify!($ty)
            );
            assert_eq!(
                shared.underruns(),
                0,
                "{} 转换路径供给充足时不该欠载",
                stringify!($ty)
            );
            out
        }};
    }

    let _ = check_format!(f32);
    let _ = check_format!(i32);
    let out = check_format!(i16);
    let _ = check_format!(u16);
    let _ = check_format!(u8);

    // 新流的增益从 0 起步，头几帧还在斜坡上——断言整段满量程会把斜坡当成故障。
    assert!(
        out[0] < i16::MAX / 2,
        "首帧应当还在斜坡上，否则就是增益突变（会爆音）"
    );
    assert!(
        out[out.len() - 1] > i16::MAX / 2,
        "斜坡收敛后，满量程输入应转换成接近满量程的整数样本"
    );
}
