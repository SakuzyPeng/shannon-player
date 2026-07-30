//! 播放引擎的外壳接线：持有引擎、注册命令、把引擎事件转成 Tauri event。
//!
//! 与曲库那半边同一套路——**逻辑全在 `shannon-audio`**（零 Tauri 依赖，可无头测试），
//! 这里只负责两件事：把命令暴露给前端、把引擎回调转成事件。
//!
//! ## 引擎为什么是懒起的
//!
//! `Engine::spawn` 会立刻起一条线程并在首次 `load` 时打开输出设备。在 `setup` 里就
//! 建好它，等于应用一启动就占用声卡——用户可能只是想整理曲库，却发现别的应用抢不到
//! 独占设备。所以推迟到第一次真正要放东西的时候。
//!
//! ## `track_id` / `load_id` 为什么要随命令进入引擎
//!
//! 引擎只认文件路径，前端的队列以曲目 ID 为键。快速连点两首歌时，前一首的进度事件
//! 会晚于后一首的装载请求；如果外壳只保存一个“最新 ID”，后一首会在引擎真正处理前一首
//! 之前覆盖它，反而给事件盖错章。因此二者作为不透明上下文随 `Load` 进入引擎，由产生事件
//! 的那个装载代际原样回带。`load_id` 还能区分同一首曲目的连续重载。

use std::sync::Mutex;

use shannon_audio::contract::PlayerEvent;
use shannon_audio::output::cpal_out::CpalOutput;
use shannon_audio::engine::LoadRequest;
use shannon_audio::{Engine, LoadContext, PlayerCmd};
use tauri::{Emitter, State};

/// 播放事件名。前端 `listen()` 用同一字符串。
pub const EVENT_PLAYER: &str = "player://event";

/// 播放器状态。
///
/// 命令经同一把锁串行投递；引擎内部同样由单线程按序处理。
#[derive(Default)]
pub struct PlayerState {
    inner: Mutex<Option<Running>>,
}

struct Running {
    engine: Engine,
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
            let engine = {
                let app = app.clone();
                Engine::spawn_stamped(Box::new(CpalOutput::new()), move |event| {
                    // 这个回调跑在引擎线程上，不能做重活。序列化 + emit 已经是上限；
                    // 装载上下文已经由引擎盖好章，这里不再读取任何“最新 ID”共享状态。
                    let payload = PlayerEvent::from_engine(&event);
                    // 发送失败（窗口已关）不该让引擎线程 panic。
                    let _ = app.emit(EVENT_PLAYER, &payload);
                })
            };
            *slot = Some(Running { engine });
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
/// `track_id` / `load_id` 只作为不透明上下文回带，引擎不解释它们。有效音量与可选的
/// 初始位置随同一条命令进入引擎，避免多次 IPC 乱序造成满音量开播或先漏出曲首 PCM。
#[tauri::command]
pub fn player_load(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    path: String,
    context: LoadContext,
    autoplay: bool,
    initial_volume: f32,
    initial_position_sec: Option<f64>,
) -> Result<(), String> {
    let slot = state.ensure(&app)?;
    let running = slot.as_ref().expect("ensure 保证已装配");
    running
        .engine
        .load_request(
            LoadRequest::new(path, autoplay, context)
                .with_volume(initial_volume)
                .with_position(initial_position_sec),
        )
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
