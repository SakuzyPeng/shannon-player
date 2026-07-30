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

use crate::loudness::LoudnessState;
use shannon_audio::contract::PlayerEvent;
use shannon_audio::engine::{LoadRequest, NextRequest};
use shannon_audio::output::cpal_out::CpalOutput;
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

/// 前端指定的「下一首」。
///
/// 队列的权威在前端，所以这里只是一条路径加一份不透明上下文；`queue_revision` 让前端
/// 认出某次切歌依据的是哪一版队列。增益仍由后端查表，与 `player_load` 同一个理由。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextTrack {
    pub path: String,
    pub context: LoadContext,
    pub queue_revision: u32,
}

impl NextTrack {
    fn into_request(self, loudness_state: &LoudnessState, loudness: bool) -> NextRequest {
        let gain = gain_for(loudness_state, loudness, &self.context, &self.path);
        NextRequest::new(self.path, self.context, self.queue_revision).with_loudness_gain(gain)
    }
}

/// 查这一首该施加多少增益。
///
/// `loudness` 是**用户的设置**（要不要归一化），具体倍率由后端查分析结果得出：
/// 目标响度与峰值上限属于播放策略，改策略不该要求前端跟着改。查不到就是 1.0，
/// 没分析过的曲目照常播放——分析永远不阻塞播放。
fn gain_for(
    loudness_state: &LoudnessState,
    loudness: bool,
    context: &LoadContext,
    path: &str,
) -> f32 {
    let gain = match (loudness, context.track_id.as_deref()) {
        (true, Some(track_id)) => loudness_state.linear_gain(track_id),
        _ => 1.0,
    };
    if gain != 1.0 {
        // 如实记录实际施加的增益：归一化是会改变听感的处理，出问题时第一个要问的
        // 就是「到底加了多少」，而这个数字在别处看不到。
        log::info!("响度归一化：{path} 施加 {:+.1} dB", 20.0 * gain.log10());
    }
    gain
}

/// 装载并播放一个文件。
///
/// `track_id` / `load_id` 只作为不透明上下文回带，引擎不解释它们。有效音量、可选的
/// 初始位置、响度增益与**下一首**随同一条命令进入引擎，避免多次 IPC 乱序造成满音量
/// 开播、先漏出曲首 PCM、前几百毫秒响一截，或者无缝换曲时好时坏。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn player_load(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    loudness_state: State<'_, LoudnessState>,
    path: String,
    context: LoadContext,
    autoplay: bool,
    initial_volume: f32,
    initial_position_sec: Option<f64>,
    loudness: bool,
    next: Option<NextTrack>,
) -> Result<(), String> {
    let gain = gain_for(&loudness_state, loudness, &context, &path);
    let next = next.map(|n| n.into_request(&loudness_state, loudness));
    let slot = state.ensure(&app)?;
    let running = slot.as_ref().expect("ensure 保证已装配");
    running
        .engine
        .load_request(
            LoadRequest::new(path, autoplay, context)
                .with_volume(initial_volume)
                .with_position(initial_position_sec)
                .with_loudness_gain(gain)
                .with_next(next),
        )
        .map_err(|e| e.to_string())
}

/// 更新无缝接续的下一首；`next` 为 `null` 表示当前这首放完就停。
///
/// **引擎还没起过时是 no-op**：那说明什么都没在放，也就没有「下一首」可言。
/// 前端每次装载都会顺带指定（见 `player_load`），这条命令只负责后续的队列变化。
#[tauri::command]
pub fn player_set_next(
    state: State<'_, PlayerState>,
    loudness_state: State<'_, LoudnessState>,
    next: Option<NextTrack>,
    loudness: bool,
) -> Result<(), String> {
    let request = next.map(|n| n.into_request(&loudness_state, loudness));
    state.with_engine(|engine| engine.set_next(request))
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
