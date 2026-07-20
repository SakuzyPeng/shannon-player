# 音频后端实现设计

## 状态

- 文档性质：实现级设计，配套并细化[音频后端架构约束](AUDIO_BACKEND_ARCHITECTURE.md)，
  不改变其决策；两者冲突时以架构约束为准，且应先修订文档再动代码。
- 设计日期：2026-07-20。
- 适用范围：实时播放引擎的工程结构、线程模型、队列交接、解码管线、前端接入、
  测试策略与实施阶段。
- 本文描述的模块、crate 与阶段均未开始实现；提及的第三方 crate 版本与 API
  在动手时需现场核实。

## 工程结构

引擎独立为 cargo workspace 成员，与 Tauri 壳解耦：

```text
src-tauri/           Tauri 壳：commands、事件桥、窗口（现状）
crates/
  shannon-audio/     播放引擎：解码、PCM 管线、输出后端，零 Tauri 依赖
  shannon-library/   曲库：扫描、标签、封面、SQLite 持久化（后期）
  shannon-import/    链接导入 ImportService（后期）
```

- `shannon-audio` 不依赖 Tauri，验收条件中的 gapless、seek、欠载压测才能以
  `cargo test` 无头执行。
- 引擎附带 example 级开发 CLI（如 `cargo run -p shannon-audio --example play <file>`），
  属于架构约束允许的开发期诊断工具，不进入正式播放路径。
- `src-tauri` 只保留胶水：命令转发、事件转发、状态桥接，不写音频逻辑。

## 引擎线程模型

控制面与数据面分离：

```text
Tauri command ──PlayerCmd──> 控制线程（状态机唯一所有者）
                                 |  管理 current / next 两个源
                                 v
              解码工作线程 ──> 无锁 SPSC 环形缓冲 ──> 输出回调
                                                        |
              控制线程 <── 无锁事件队列（切歌边界、欠载等标记）
                 |
                 v
       对外事件：Progress / TrackChanged / StateChanged / Error / DeviceListChanged
```

- 控制线程是 `Idle / Loading / Playing / Paused / Error` 状态机的唯一所有者，经消息
  通道接收命令。Tauri command 只投递消息并立即返回，结果一律走事件，音频不进入
  Tauri 的 async 运行时。
- 解码工作线程串联容器读取、解码与 PCM 管线，向环形缓冲推入帧。
- 输出回调只从环形缓冲消费；越过切歌边界、发生欠载等时刻仅向无锁事件队列压入标记，
  对外事件统一由控制线程消费标记后发出。

实时纪律清单（细化架构约束实时链路不变量第 3 条）：

- 回调可触碰的共享结构仅限：SPSC 消费端、原子量（音量、静音、generation、欠载计数）、
  无锁事件队列的生产端。
- 回调内禁止 Mutex（包括“只锁一瞬”）、文件与网络 I/O、动态分配、格式化日志。
- 欠载计数为原子累加，经 stats 查询接口暴露，对应架构约束验收条件第 5 条。

seek 与切歌沿用研究笔记的 generation 方案：控制线程递增原子 generation，解码线程丢弃
在途数据、清空环形缓冲并从新位置重填，达到预缓冲阈值（沿用概念验证的 300 ms）后恢复
消费。

generation 解决音频数据的新旧，不解决命令与事件的配对：seek 等异步命令另携带自增
request_id，进度与完成事件回带 generation 与最近 request_id，前端据此丢弃过期回报，
避免连续拖动进度条时旧回报把界面拉回旧位置。队列侧的对应机制是 queueRevision，见
「队列归属与切歌交接」。

## 播放位置与进度事件

- 播放位置 = 输出回调累计消费帧数 − 输出设备延迟，符合架构约束不变量第 6 条，
  不以 JavaScript 定时器为事实来源。
- 进度事件按约 5 Hz 推送；UI 在事件间用 requestAnimationFrame 插值，事件只做重锚定。
- 歌词逐字同步必须使用扣除设备延迟后的位置。共享模式输出延迟普遍达数十毫秒，
  不补偿会让逐字高亮系统性偏早。

## 队列归属与切歌交接

队列权威保留在前端 Zustand store，引擎只建模 current 与 next 两个槽位。

前置事实：当前 store 的 shuffle 只是布尔开关，`next()` 一律顺序推进，尚不存在真正的
“下一首”算法。因此“队列逻辑留在前端”不是迁移豁免，而是阶段 0 的实作任务：store 需
先提供统一的后继计算（shuffle 顺序表、repeat 语义、user/auto 入队优先级归一），
`set_next` 协议以其输出为输入，本文不假设该逻辑已经存在。

- store 在每次队列、循环或随机状态变化时重算下一首，并调用
  `player_set_next(uid, path, queueRevision)`。
- store 每次队列相关变更递增 queueRevision；引擎记录最近收到的值，并在
  `TrackChanged { fromUid, toUid, queueRevision }` 中回带。store 只接受
  queueRevision 等于当前值的事件，过期事件直接丢弃——否则快速连续改队列时，
  会按旧队列的边界错误推进 currentIndex。
- queueRevision 不只过滤事件，必须同时使已预解码的旧 next 音频失效，二选一实现：
  next 使用独立的可替换预缓冲，替换时整体丢弃旧 next（主缓冲只含 current，旧缓冲由
  解码或控制线程回收，释放不得发生在回调内）；或 PCM 块携带 revision，消费端在边界
  处校验并丢弃过期块。无论何种实现，输出端绝不能消费过期 revision 的 next PCM——
  否则旧曲仍会真正发声，而前端已按 revision 丢弃对应 TrackChanged，造成音频与界面
  分裂。
- 边界到达而新 next 尚未达到预缓冲阈值时，走 seek 的再缓冲路径（短暂停顿后续播）：
  宁可出现可解释的停顿，不播放过期音频。
- revision 只作用于 next 槽位的音频与边界标记；current 的 PCM 不因改队列失效，仅由
  seek 与切歌的 generation 控制。
- 引擎在边界处帧级精确续播（满足不变量第 7 条的 gapless 要求）。
- repeat-one 即 `set_next` 指向当前曲目自身。
- `TrackChanged` 必须在消费端越过边界帧时判定，而不是解码器切源时——解码可领先播放
  数秒，按解码时机发事件会让界面提前切换曲目信息。实现上在环形缓冲维护
  (generation, queueRevision, 边界帧号) 标记队列；输出回调越界时仅向无锁事件队列
  压入标记，由控制线程消费后对外发事件（revision 过期的标记直接丢弃），回调自身
  不做任何事件发送。

后期曲库与队列若整体下沉 Rust，再重估队列归属；当前阶段不做该迁移。

## 解码管线

架构约束分层图中 `SymphoniaDecoder` 与 `CodecSpecificDecoder` 的落地形态合并为一条
Symphonia 管线加自定义解码器注册：

- 容器探测、解复用、元数据、seek 统一走 Symphonia；Opus 等其尚未覆盖的编码以自定义
  Decoder 注册进 CodecRegistry，复用其 MKV/WebM/Ogg 解复用器，不另起容器解析栈。
- Opus 解码采用 libopus 绑定（`opus` / `audiopus` 选型时定），seek 需处理 pre-roll。
- 该方案与架构约束“解码后端选择”第 3 条一致，是专用进程内解码器的具体集成方式；
  后端分类实际收敛为三类：Symphonia（含插件解码器）、可选 libav、原生空间。

数据契约：

- 全链交换格式为 f32 交错 PCM，以每声道时间帧计数（不变量第 1 条）。
- 源规格自解码起点即携带 `layout` 与 `layout_source`（显式标签 / 声道数推断 / 默认），
  对应固定多声道笔记对布局置信度的要求；溯源事后补录代价大，必须一开始就进类型。

管线顺序：

```text
解码 -> gapless 裁剪 -> ReplayGain / 音量 -> 重采样 -> 声道映射 -> 环形缓冲
```

- gapless 裁剪使用容器与码流提供的 encoder delay/padding（Symphonia 的
  `enable_gapless` 或等价机制）。
- 音量在引擎内做线性增益，变化带 10–20 ms 斜坡；不做斜坡会产生可闻爆音，斜坡行为
  纳入验收。
- 暂停 = 斜坡到零后回调停止从环形缓冲取帧，但继续向设备写零帧，输出流与设备时钟
  保持活跃；只有停止播放或设备切换才拆除输出流。直接停掉回调消费会让恢复播放经历
  设备重启延迟，也让基于消费帧数的位置时钟失去参照。
- 重采样用 rubato，仅在源采样率不等于设备采样率时启用，并在 stats 中如实标记
  “已重采样”，对应架构约束对 bit-perfect 措辞的限制。

## 输出后端

- `OutputBackend` trait 首版即定（能力协商 / 启动 / 停止 / 能力上报），WASAPI 独占与
  Windows 空间输出后期以新实现插入，不改引擎。
- 起步实现为 CPAL 共享模式。不得假设设备支持 f32：按设备实际支持的配置协商采样格式
  与采样率；引擎内部全链保持 f32，由输出后端在设备边界完成到协商格式的样本转换，
  转换在回调内进行且无分配。
- 增加 `NullOutput`：供无声卡 CI 与集成测试使用，也是前端 Mock 的行为参照。
- CPAL 的设备热插拔通知跨平台不完整，按架构约束划入平台后端职责，各平台以
  IMMNotificationClient、CoreAudio property listener 等机制补齐。

## 前端接入

- 前端定义 `EngineAdapter` 接口，两个实现按运行环境选择：`TauriEngine`
  （invoke + 事件订阅）与 `MockEngine`（现 `tick()` 占位时钟改造）。浏览器
  `:1420` 的 Playwright 验证流程因此不受引擎接入影响。
- 引擎事件走单通道 tagged union，类型镜像进 `src/types/player.ts`；
  `PlaybackProgress.bufferedSec` 由环形缓冲水位换算。
- 错误事件结构对齐架构约束验收条件第 2 条：容器、编码、失败阶段、可展示信息。

## 错误与降级契约

- 曲目中途解码失败：发结构化 Error 事件，默认自动跳下一首并给非阻塞提示，不静默停止
  也不静默换后端。
- 以 uid × generation 为键限制自动重试次数，避免单曲循环遇上损坏文件形成无限错误循环。
- 端点或格式能力不满足时报明确原因，沿用架构约束：不静默降级。

## 测试策略

`shannon-audio` 与 Tauri 无关，测试全部无头运行：

- 语料测试：连续正弦波切成两个文件分别编码，断言播放边界处相位连续（误差小于阈值），
  验证 gapless；对应验收条件第 6 条。
- seek 等价测试：任意位置 seek 后的解码输出等于从头解码的对应后缀。
- 欠载测试：慢消费者模拟，断言欠载计数与恢复行为；对应验收条件第 5 条。
- next 替换测试：next 已预解码后修改队列，断言输出不出现旧 next 的样本，且
  TrackChanged 回带新 queueRevision；对应「队列归属与切歌交接」的失效约束。
- AAC / ALAC 等“不能预先承诺 gapless”的格式，以语料测试产出兼容性结论后再更新格式
  范围表，不凭实现推断。
- 测试语料由脚本在开发期生成（属架构约束允许的开发期工具），不提交大体积二进制。

## 曲库扫描选型

- 标签与封面读取用 lofty（覆盖 ID3v2 / Vorbis Comment / MP4 / APE、封面与
  ReplayGain 标签），持久化用 SQLite（rusqlite）。
- 扫描期即入库：布局值与布局来源、gapless 元数据（delay/padding）、ReplayGain 值，
  播放期不再重复解析。
- 满足架构约束进程边界表：扫描不创建子进程。

## 对架构约束待定项的建议

| 待定项 | 建议 |
| --- | --- |
| 最低 Rust 版本 | 立即提升至 1.85 或更高；桌面应用自带二进制、无下游 crate 消费者，成本仅为本地与 CI 工具链，为 Symphonia 0.6 评估扫清前置 |
| Symphonia 版本与 feature 集 | 以语料测试为基线在 0.5 系与 0.6 间实测选择；自定义解码器注册 API 在 0.6 中需现场核实 |
| Opus 解码器选型 | libopus 绑定包装为 Symphonia 自定义 Decoder，复用其解复用器（见“解码管线”） |
| 是否需要 `ffmpeg-next` | 维持架构约束原判：仅当插件式解码仍覆盖不了且格式收益明确时评估 |
| CPAL 与平台后端边界 | 以 `OutputBackend` trait 为界：共享模式归 CPAL；独占、直通、空间、热插拔归平台实现 |
| 导入用 FFmpeg 裁剪与打包 | 维持待定，属 ImportService（阶段 3）前的决策点 |

## 实施阶段

| 阶段 | 内容 | 出口条件 |
| --- | --- | --- |
| 0 | workspace 拆分、引擎骨架、CPAL 输出（设备格式协商）、Symphonia FLAC/MP3/WAV、播放/暂停/seek/音量、事件桥、MockEngine、store 后继算法与 queueRevision | 占位时钟 `tick()` 退役，浏览器验证流程不回退 |
| 1 | gapless、ReplayGain、重采样、设备切换、stats、语料测试 | 验收条件第 5、6 条的无头测试常态化 |
| 2 | 曲库扫描（lofty + SQLite）替换种子数据 | 前端不再读 `src/data/library.ts` 种子 |
| 3 | Opus 插件解码、ImportService | 架构约束第一阶段格式表除空间路径外全覆盖 |
| 4 | 平台独占输出 | 独占/共享切换走显式状态机 |
| 5 | Windows 空间后端 | 两份研究笔记的验证方法在产品内可复现 |

## 当前未覆盖

- 均衡器等重型 DSP 的具体设计。
- 曲库数据库 schema 与迁移策略。
- 空间音频能力在设置界面的具体呈现。
- 导入任务的界面与进度模型细节。
