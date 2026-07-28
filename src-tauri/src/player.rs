//! 播放引擎的外壳接线：持有引擎、注册命令、把引擎事件转成 Tauri event。
//!
//! 与曲库那半边同一套路——**逻辑全在 `shannon-audio`**（零 Tauri 依赖，可无头测试），
//! 这里只负责三件事：把命令暴露给前端、把引擎回调转成事件、记住「当前装载的是哪首」。
//!
//! ## 引擎为什么是懒起的
//!
//! `Engine::spawn` 会立刻起一条线程并在首次 `load` 时打开输出设备。在 `setup` 里就
//! 建好它，等于应用一启动就占用声卡——用户可能只是想整理曲库，却发现别的应用抢不到
//! 独占设备。所以推迟到第一次真正要放东西的时候。
//!
//! ## `track_id` 为什么要外壳记
//!
//! 引擎只认文件路径，前端的队列以曲目 ID 为键。快速连点两首歌时，前一首的进度事件
//! 会晚于后一首的装载到达；不带 ID 的话这些迟到事件会被记到新曲目头上，表现为进度条
//! 跳一下。让外壳在装载时记下 ID 并给每个事件盖章，前端就能把过期事件直接丢掉。

use std::sync::{Arc, Mutex};

use shannon_audio::contract::PlayerEvent;
use shannon_audio::output::cpal_out::CpalOutput;
use shannon_audio::{Engine, PlayerCmd};
use tauri::{Emitter, State};

/// 播放事件名。前端 `listen()` 用同一字符串。
pub const EVENT_PLAYER: &str = "player://event";

/// 播放器状态。
///
/// `engine` 与 `track_id` 分开加锁会引入一个真实的竞态：装载与「记下 ID」之间若有
/// 别的命令挤进来，事件就会盖错章。合成一把锁，代价是命令串行——而命令本来就该串行，
/// 引擎内部也是单线程按序处理的。
#[derive(Default)]
pub struct PlayerState {
    inner: Mutex<Option<Running>>,
}

struct Running {
    engine: Engine,
    /// 当前装载曲目的 ID，由事件回调共享读取。
    track_id: Arc<Mutex<Option<String>>>,
}

impl PlayerState {
    /// 取已有引擎，没有就现起一个。
    ///
    /// 起引擎会打开音频设备，因此**不在应用启动时做**——见模块头注释。
    fn ensure(
        &self,
        app: &tauri::AppHandle,
    ) -> Result<std::sync::MutexGuard<'_, Option<Running>>, String> {
        let mut slot = self.inner.lock().map_err(|e| e.to_string())?;
        if slot.is_none() {
            let track_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let engine = {
                let app = app.clone();
                let track_id = track_id.clone();
                Engine::spawn(Box::new(CpalOutput::new()), move |event| {
                    // 这个回调跑在引擎线程上，不能做重活。序列化 + emit 已经是上限；
                    // 锁只在读一个 Option<String> 的瞬间持有。
                    let id = track_id.lock().ok().and_then(|g| g.clone());
                    let payload = PlayerEvent::from_engine(&event, id);
                    // 发送失败（窗口已关）不该让引擎线程 panic。
                    let _ = app.emit(EVENT_PLAYER, &payload);
                })
            };
            *slot = Some(Running { engine, track_id });
        }
        Ok(slot)
    }

    /// 对已存在的引擎发命令。引擎还没起过就是 no-op——
    /// 「暂停一个从没开始过的播放」不是错误，不该让前端收到失败。
    fn with_engine<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&Engine) -> shannon_audio::Result<()>,
    {
        let slot = self.inner.lock().map_err(|e| e.to_string())?;
        let Some(running) = slot.as_ref() else {
            return Ok(());
        };
        f(&running.engine).map_err(|e| e.to_string())
    }
}

/// 装载并播放一个文件。
///
/// `track_id` 只用来给事件盖章，引擎不认识它。
#[tauri::command]
pub fn player_load(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    path: String,
    track_id: String,
    autoplay: bool,
) -> Result<(), String> {
    let slot = state.ensure(&app)?;
    let running = slot.as_ref().expect("ensure 保证已装配");
    // 先记 ID 再发命令：反过来的话，装载事件会带着上一首的 ID 发出去。
    *running.track_id.lock().map_err(|e| e.to_string())? = Some(track_id);
    running
        .engine
        .load(path, autoplay)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_play(state: State<'_, PlayerState>) -> Result<(), String> {
    state.with_engine(|e| e.play())
}

#[tauri::command]
pub fn player_pause(state: State<'_, PlayerState>) -> Result<(), String> {
    state.with_engine(|e| e.pause())
}

#[tauri::command]
pub fn player_seek(state: State<'_, PlayerState>, position_sec: f64) -> Result<(), String> {
    state.with_engine(|e| e.seek(position_sec))
}

/// 设音量。**引擎还没起时也要记住**——否则用户开播前调好的音量会被忽略，
/// 第一首歌以满音量炸出来。这里的做法是照常起引擎（它不占设备，直到 load）。
#[tauri::command]
pub fn player_set_volume(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    volume: f32,
) -> Result<(), String> {
    let slot = state.ensure(&app)?;
    slot.as_ref()
        .expect("ensure 保证已装配")
        .engine
        .set_volume(volume)
        .map_err(|e| e.to_string())
}

/// 停止并卸载当前音源，释放输出设备。
#[tauri::command]
pub fn player_stop(state: State<'_, PlayerState>) -> Result<(), String> {
    state.with_engine(|e| e.send(PlayerCmd::Stop))
}
