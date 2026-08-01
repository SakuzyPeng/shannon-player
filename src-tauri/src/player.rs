//! 播放引擎的外壳接线：持有引擎、注册命令、把引擎事件转成 Tauri event。
//!
//! 与曲库那半边同一套路——**逻辑全在 `shannon-audio`**（零 Tauri 依赖，可无头测试），
//! 这里只负责两件事：把命令暴露给前端、把引擎回调转成事件。
//!
//! ## 引擎为什么是懒起的
//!
//! `Engine::spawn` 只会立刻起控制线程，输出设备到首次 `load` 才打开。因此这里按需创建
//! 引擎以免常驻一条无用线程；播放前的音量或 next 更新可以先创建它，但仍不会占用声卡。
//!
//! ## `track_id` / `load_id` 为什么要随命令进入引擎
//!
//! 引擎只认文件路径，前端的队列以曲目 ID 为键。快速连点两首歌时，前一首的进度事件
//! 会晚于后一首的装载请求；如果外壳只保存一个“最新 ID”，后一首会在引擎真正处理前一首
//! 之前覆盖它，反而给事件盖错章。因此二者作为不透明上下文随 `Load` 进入引擎，由产生事件
//! 的那个装载代际原样回带。`load_id` 还能区分同一首曲目的连续重载。

use std::sync::Mutex;

use crate::loudness::LoudnessState;
use shannon_audio::contract::{AudioDeviceInfo, PlayerEvent};
use shannon_audio::engine::{LoadRequest, NextRequest};
use shannon_audio::output::cpal_out::{CpalDevices, CpalOutput};
use shannon_audio::output::DeviceEnumerator;
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
    /// 取已有引擎，没有就现起一个控制线程。
    ///
    /// 这里只创建控制线程，输出设备仍到首次 `Load` 才打开，因此 `SetVolume` 与可能
    /// 先于 `Load` 到达的 `SetNext` 都可以安全调用它，不会提前占用声卡。
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
/// 队列的权威在前端，所以这里只是一条路径加一份不透明上下文；队列版本与播放链 ID
/// 由命令顶层携带，即使 `next = null` 也不会丢。增益仍由后端查表。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextTrack {
    pub path: String,
    pub context: LoadContext,
}

impl NextTrack {
    fn into_request(self, loudness_state: &LoudnessState, loudness: bool) -> NextRequest {
        let gain = gain_for(loudness_state, loudness, &self.context, &self.path);
        NextRequest::new(self.path, self.context).with_loudness_gain(gain)
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
    queue_revision: u32,
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
                .with_next(next, queue_revision),
        )
        .map_err(|e| e.to_string())
}

/// 更新无缝接续的下一首；`next` 为 `null` 表示当前这首放完就停。
///
/// 这里即使在首次 `Load` 之前也要创建控制线程并投递：多条 Tauri invoke 的到达顺序
/// 没有保证，先到的更新必须由引擎按 chain 暂存，不能因为设备尚未打开就静默丢掉。
#[tauri::command]
pub fn player_set_next(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    loudness_state: State<'_, LoudnessState>,
    next: Option<NextTrack>,
    loudness: bool,
    chain_id: String,
    queue_revision: u32,
) -> Result<(), String> {
    let request = next.map(|n| n.into_request(&loudness_state, loudness));
    let slot = state.ensure(&app)?;
    slot.as_ref()
        .expect("ensure 保证已装配")
        .engine
        .set_next(chain_id, request, queue_revision)
        .map_err(|e| e.to_string())
}

/// 列出可用的输出端点。
///
/// **不经引擎**：枚举是查询不是命令，既不打开设备也不碰播放状态，没有理由排进引擎那条
/// 串行命令队列（见 `DeviceEnumerator` 的说明）。因此这条命令也不会顺带把引擎起起来。
///
/// **每次都重新问系统**：设备会插拔，缓存一份只会让菜单显示已经拔掉的耳机。
#[tauri::command]
pub fn player_list_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    CpalDevices
        .devices()
        .map(|devices| devices.iter().map(AudioDeviceInfo::from).collect())
        .map_err(|e| e.to_string())
}

/// 选定输出端点；`null` = 跟随系统默认。
///
/// 与 `player_set_next` 同理，引擎还没起过时也要创建控制线程并投递：用户可能在开播前
/// 就在设置里选好了设备。输出设备仍到首次 `Load` 才打开，记一个偏好不占声卡。
///
/// 选中的端点用不了时**不回落默认**，而是回一条 `deviceRejected` 事件并继续在原端点上
/// 放——静默换一台意味着用户以为声音在 DAC 上、实际从笔记本喇叭里出来。
#[tauri::command]
pub fn player_set_device(
    app: tauri::AppHandle,
    state: State<'_, PlayerState>,
    device_id: Option<String>,
    device_revision: u64,
) -> Result<(), String> {
    let slot = state.ensure(&app)?;
    slot.as_ref()
        .expect("ensure 保证已装配")
        .engine
        .set_device(device_id, device_revision)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端发过来的形状。
    ///
    /// 这类边界只能靠测试钉住：字段名写错不会有任何编译期报错，运行期表现是
    /// 「无缝换曲静默失效」——`next` 被当成缺席，引擎照旧在曲目之间停一下，
    /// 而日志里什么都没有。同类教训见 `PlayerEvent` 那条 `rename_all_fields`。
    #[test]
    fn next_track_accepts_the_frontend_shape() {
        let json = r#"{
            "path": "/音乐/专辑/02.flac",
            "context": { "trackId": "t-1", "loadId": "load-9" }
        }"#;
        let next: NextTrack = serde_json::from_str(json).expect("前端形状必须能解开");
        assert_eq!(next.path, "/音乐/专辑/02.flac");
        assert_eq!(next.context.track_id.as_deref(), Some("t-1"));
        assert_eq!(next.context.load_id, "load-9");
    }

    #[test]
    fn no_next_track_is_expressible() {
        // 「放完就停」必须能明说：不说的话引擎会一直接着上次指定的那首，
        // 用户删掉队尾之后反而会绕回去。
        let absent: Option<NextTrack> = serde_json::from_str("null").expect("null 要能解成 None");
        assert!(absent.is_none());
    }
}
