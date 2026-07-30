//! 分析队列与后台 worker。
//!
//! ## 为什么不是「批量全扫」
//!
//! 批量思维把成本表述成「用户要等多久」，而那个数字在不同机器上差一个数量级，
//! 没法预告（本机 131 秒的活，老双核笔记本可能是 20 分钟）。正确的约束不是全库总时间，
//! 而是**单首是否快于实时播放**——实测真实曲库 383× 实时，两个数量级的余量。
//!
//! 于是「预取下一首」与「分析全库」合并成同一件事：一个按播放顺序排优先级的队列。
//! 跑空了就是全库分析完成，它不再是一个需要用户去点的按钮，只是队列的一个状态。
//! 跳到队列中段也不再是特例——那一段大概率早就算过了。
//!
//! ## 一个后台 worker
//!
//! 阶段 1 固定 1 个，并降为后台 QoS（见 [`super::qos`]）。已有的多线程加速比实测测的是
//! **纯解码**，不能证明 EBU R128 + 真峰值的吞吐与播放共存；要提高默认值得用
//! `bench_decode --loudness` 在目标平台重跑 1/2/4/8 线程并同时跑欠载测试，两项缺一不可。
//! 也**不做成用户配置项**：用户答不上「响度分析该用几个线程」，要回答它得同时知道内存
//! 带宽、QoS 调度、电池影响与存储介质。
//!
//! ## 瞬态错误不写成结论
//!
//! 文件读不到、解码失败一律**不记**任何东西，下次重排队列时自然重试。把一次网络盘
//! 掉线写成永久结论，等于让那首歌再也不会被分析。可缓存的只有
//! `Unmeasurable` / `UnsupportedLayout` 这类确定状态。
//!
//! ## 锁的顺序
//!
//! 需要同时看结果与队列时，**先取 `store` 再取 `queue`，并且不在持有 `queue` 时落盘**
//! ——写盘是毫秒级的，攥着队列锁写会让「切歌 → 重排优先级」跟着卡住。

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;

use super::qos;
use super::store::LoudnessStore;
use super::LoudnessOutcome;

/// 攒够这么多条新结论就落一次盘。
///
/// 逐条写没有正确性问题（写入是原子的），但全库 950 首会写 950 次、每次都比上次更大；
/// 攒批把它降到几十次。队列跑空与退出时另有一次收尾，所以攒批不会留下未落盘的尾巴。
const SAVE_EVERY: usize = 16;

/// 一件待分析的活。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisItem {
    /// 曲目 ID（内容哈希），也是结果的键。
    pub track_id: String,
    pub path: PathBuf,
}

/// 后台响度分析服务。
///
/// 持有分析结果，并按调用方给的顺序在后台把它们补齐。播放路径只向它**查**增益，
/// 查不到就不归一化——分析永远不阻塞播放。
pub struct LoudnessService {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

struct Shared {
    store: RwLock<LoudnessStore>,
    store_path: PathBuf,
    queue: Mutex<Queue>,
    wake: Condvar,
    /// 收到停止请求。解码循环每块都看它一眼，因此退出不必等一整首分析完。
    stop: AtomicBool,
    /// 每次整体替换队列都递增。worker 的解码闭包逐块核对，空队列因而也能立即取消
    /// 手上正在跑的那首，而不只是阻止下一首启动。
    queue_generation: AtomicU64,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<AnalysisItem>,
    /// `pending` 属于哪次整体替换。与原子代际分开保存，使 worker 在新队列尚未组装完时
    /// 偶然取到旧项，也仍会带着旧代际并立即取消。
    generation: u64,
    /// worker 手上正在分析的那首。
    ///
    /// 算进 [`LoudnessService::pending`] 里：从队列取走到结论落进 store 之间还有一段时间，
    /// 不算它的话「还剩几首」会在最后一首上提前归零——对进度显示是差一格，
    /// 对「等它跑完再查结果」则是彻底的竞态。
    in_flight: bool,
    stop: bool,
}

impl LoudnessService {
    /// 读入已有结果并起一个后台 worker。队列初始为空，等调用方按播放顺序喂进来。
    pub fn spawn(store_path: PathBuf) -> Self {
        let shared = Arc::new(Shared::new(store_path));
        let worker = {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("shannon-loudness".into())
                .spawn(move || run(shared))
                .ok()
        };
        Self { shared, worker }
    }

    /// 按播放顺序重排待分析队列，返回还剩多少首要分析。
    ///
    /// 调用方给什么顺序就按什么顺序做，优先级就是「距当前播放位置的远近」：切歌、改队列、
    /// 开随机之后重新喂一遍即可。已经有当前版本结论的曲目会被滤掉，
    /// 「现在全部分析」这类意图级动作也只是把整库按序喂进来，不额外提高并发。
    pub fn set_queue(&self, items: impl IntoIterator<Item = AnalysisItem>) -> usize {
        let remaining = self.shared.replace_queue(items);
        self.shared.wake.notify_all();
        remaining
    }

    /// 该乘到这首曲目 PCM 上的线性增益。没分析过就是 1.0（不处理）。
    pub fn linear_gain(&self, track_id: &str) -> f32 {
        self.shared
            .store
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .linear_gain(track_id)
    }

    /// 查结论本身（诊断与测试用；播放路径只需要 [`Self::linear_gain`]）。
    pub fn outcome(&self, track_id: &str) -> Option<LoudnessOutcome> {
        self.shared
            .store
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(track_id)
    }

    /// 还有多少首没分析完（含正在分析的那首）。0 表示已知范围内全部完成。
    pub fn pending(&self) -> usize {
        let queue = lock(&self.shared.queue);
        queue.pending.len() + usize::from(queue.in_flight)
    }
}

impl Drop for LoudnessService {
    /// 退出时停 worker 并落盘。
    ///
    /// 停止标志同时被解码循环轮询，所以这里最多等一块 PCM 的时间（毫秒级），
    /// 而不是等当前这首整曲分析完（一首 4 分钟的曲子约 0.6 秒——关窗时的 0.6 秒是看得见的）。
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        lock(&self.shared.queue).stop = true;
        self.shared.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// worker 从队列取到的下一步动作。
enum Step {
    Analyze(AnalysisItem, u64),
    /// 队列空了且有未落盘的结论——**在锁外**落盘，然后回来接着等。
    Flush,
    /// 已经等到了新活或停止请求，回到循环重新判断。
    Woke,
    Stop,
}

impl Shared {
    fn new(store_path: PathBuf) -> Self {
        Self {
            // 读不出就当没分析过：这份数据可重建，后台会自己补回来。
            store: RwLock::new(LoudnessStore::load(&store_path).unwrap_or_default()),
            store_path,
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            queue_generation: AtomicU64::new(0),
        }
    }

    /// 整体**替换**待分析队列，返回还剩多少首。
    ///
    /// 替换而不是追加：调用方给的是一份「当前该按什么顺序分析」的完整意见，
    /// 追加会让上一次的顺序残留在前面，越排越不像播放顺序。
    fn replace_queue(&self, items: impl IntoIterator<Item = AnalysisItem>) -> usize {
        // 先发布新代际，让正在分析的旧任务立即看见取消；组装新队列期间若 worker
        // 取到旧 pending，它携带的仍是 Queue 里的旧代际，不会误冒充新任务。
        let generation = self.queue_generation.fetch_add(1, Ordering::AcqRel) + 1;
        // 先看结果再动队列（见模块文档的锁顺序），读锁在这里就放掉。
        let pending: VecDeque<AnalysisItem> = {
            let store = self.store.read().unwrap_or_else(|e| e.into_inner());
            let mut seen = HashSet::new();
            items
                .into_iter()
                .filter(|item| seen.insert(item.track_id.clone()))
                .filter(|item| store.get(&item.track_id).is_none())
                .collect()
        };
        let remaining = pending.len();
        let mut queue = lock(&self.queue);
        queue.pending = pending;
        queue.generation = generation;
        remaining
    }

    fn is_generation_current(&self, generation: u64) -> bool {
        self.queue_generation.load(Ordering::Acquire) == generation
    }

    fn next_step(&self, has_unsaved: bool) -> Step {
        let mut queue = lock(&self.queue);
        if queue.stop {
            return Step::Stop;
        }
        if let Some(item) = queue.pending.pop_front() {
            queue.in_flight = true;
            return Step::Analyze(item, queue.generation);
        }
        if has_unsaved {
            return Step::Flush;
        }
        let _unused = self.wake.wait(queue).unwrap_or_else(|e| e.into_inner());
        Step::Woke
    }

    fn save(&self) {
        let mut store = self.store.write().unwrap_or_else(|e| e.into_inner());
        if !store.is_dirty() {
            return;
        }
        // 写不出去（磁盘满、目录没权限）不该让分析停摆：内存里的结论仍然有效，
        // 只是这一程结束后要重算。这条路径没有用户可采取的行动，因此不上报。
        let _ = store.save(&self.store_path);
    }
}

fn run(shared: Arc<Shared>) {
    qos::apply_background();
    let mut unsaved = 0usize;
    loop {
        match shared.next_step(unsaved > 0) {
            Step::Analyze(item, generation) => {
                let produced = analyze_one(&shared, &item, generation);
                lock(&shared.queue).in_flight = false;
                if produced {
                    unsaved += 1;
                }
                if unsaved >= SAVE_EVERY {
                    shared.save();
                    unsaved = 0;
                }
            }
            Step::Flush => {
                shared.save();
                unsaved = 0;
            }
            Step::Woke => {}
            Step::Stop => break,
        }
    }
    if unsaved > 0 {
        shared.save();
    }
}

/// 分析一首，返回是否产生了新结论。
fn analyze_one(shared: &Shared, item: &AnalysisItem, generation: u64) -> bool {
    let keep_going =
        || !shared.stop.load(Ordering::Relaxed) && shared.is_generation_current(generation);
    match super::analyze_file_interruptible(&item.path, keep_going) {
        // 被打断：没有结论可存，下次重排队列时自然重来。
        Ok(None) => false,
        Ok(Some(outcome)) => {
            let mut store = shared.store.write().unwrap_or_else(|e| e.into_inner());
            // 最后一块解码结束到拿到写锁之间也可能发生重排；在锁内再核对一次，
            // 保证取消任务绝不把半旧的结论提交进新代际。
            if !shared.is_generation_current(generation) {
                return false;
            }
            store.set(item.track_id.clone(), outcome);
            true
        }
        // 瞬态错误不写成永久结论，见模块文档。
        Err(_) => false,
    }
}

/// 取锁，中毒也照常继续。
///
/// worker 若在某首曲目上 panic，正确的代价是「那首没分析成」，而不是整个响度功能
/// 从此瘫掉——被污染的只有那一次分析的局部状态，队列本身仍然是自洽的。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// 队列本身的用例。
///
/// 直接对 `Shared` 做，**不起 worker**：起了的话它会立刻把队列吃掉，
/// 「第二件是不是 b」这种断言就成了跟线程赛跑。端到端的行为另有集成测试
/// （`audio/tests/loudness_queue.rs`）。
#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> AnalysisItem {
        AnalysisItem {
            track_id: id.into(),
            path: PathBuf::from(format!("/nowhere/{id}.flac")),
        }
    }

    fn shared() -> Shared {
        Shared::new(
            std::env::temp_dir().join(format!("shannon_loudness_queue_{}", std::process::id())),
        )
    }

    /// 最多取 `max` 件，队列空了就停。
    ///
    /// 传 `has_unsaved = true` 是**为了不阻塞**：空队列且无待写内容时 `next_step` 会等在
    /// 条件变量上（那正是 worker 该做的事），测试里等于挂死。
    fn taken(shared: &Shared, max: usize) -> Vec<String> {
        let mut out = Vec::new();
        for _ in 0..max {
            match shared.next_step(true) {
                Step::Analyze(item, _) => out.push(item.track_id),
                _ => break,
            }
        }
        out
    }

    #[test]
    fn queue_is_consumed_in_the_order_given() {
        // 顺序就是优先级：调用方按「距当前播放位置的远近」排好，这里不再自作主张。
        let shared = shared();
        assert_eq!(shared.replace_queue([item("a"), item("b"), item("c")]), 3);
        assert_eq!(taken(&shared, 3), ["a", "b", "c"]);
    }

    #[test]
    fn a_new_order_replaces_the_old_one_instead_of_queueing_behind_it() {
        // 切歌后重排：新顺序必须立刻生效。若是追加，用户刚跳过去的那首会排在
        // 上一份顺序的整条队伍后面——预取也就失去了意义。
        let shared = shared();
        shared.replace_queue([item("a"), item("b"), item("c")]);
        assert_eq!(shared.replace_queue([item("c"), item("a")]), 2);
        assert_eq!(taken(&shared, 3), ["c", "a"], "旧顺序不该有残留");
    }

    #[test]
    fn analyzed_tracks_and_duplicates_are_filtered_out() {
        // 队列允许同一首歌多次入队（播放队列本来就允许），但分析一次就够。
        let shared = shared();
        shared
            .store
            .write()
            .unwrap()
            .set("done", LoudnessOutcome::Unmeasurable);
        assert_eq!(
            shared.replace_queue([item("done"), item("a"), item("a")]),
            1
        );
        assert_eq!(taken(&shared, 2), ["a"]);
    }

    #[test]
    fn draining_the_queue_asks_for_a_flush_exactly_once() {
        // 队列跑空正是攒批落盘的时机；但「空」会被反复看到，不能每看一次写一次盘。
        let shared = shared();
        shared.replace_queue([item("a")]);
        assert!(matches!(shared.next_step(true), Step::Analyze(_, _)));
        assert!(matches!(shared.next_step(true), Step::Flush), "空了要落盘");
        // 落完盘后 worker 会以 has_unsaved=false 再问一次，那一次才是真的等。
        lock(&shared.queue).stop = true;
        assert!(matches!(shared.next_step(false), Step::Stop));
    }

    #[test]
    fn stop_wins_over_pending_work() {
        // 退出时不该再开一首新的分析：那会让关窗多等一整首的时间。
        let shared = shared();
        shared.replace_queue([item("a"), item("b")]);
        lock(&shared.queue).stop = true;
        assert!(matches!(shared.next_step(false), Step::Stop));
    }

    #[test]
    fn replacing_with_an_empty_queue_cancels_the_in_flight_generation() {
        let shared = shared();
        shared.replace_queue([item("a")]);
        let generation = match shared.next_step(true) {
            Step::Analyze(_, generation) => generation,
            _ => panic!("应当取到分析任务"),
        };
        assert!(shared.is_generation_current(generation));

        shared.replace_queue([]);
        assert!(
            !shared.is_generation_current(generation),
            "关掉功能推入空队列时，手上正在解码的任务也必须立即失效"
        );
    }
}
