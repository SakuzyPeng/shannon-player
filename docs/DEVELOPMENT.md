# 开发文档

面向贡献者的技术说明。用户向的介绍见根目录 [README](../README.md)。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 桌面壳 | Tauri 2（macOS 用系统标题栏叠加内容，Windows / Linux 无边框 + 自绘交通灯） |
| 前端 | React 19 + TypeScript + Vite |
| 样式 | Tailwind v4 + 设计 Token（CSS 变量，浅/深/系统三态） |
| 状态 | Zustand（强类型播放器领域模型：队列 / 进度 / 音频设备 / 循环模式） |
| 动画 | Framer Motion |
| 菜单 | Radix UI（右键菜单 / 下拉菜单，键盘导航与无障碍开箱即用） |
| 国际化 | 自建类型安全 i18n（当前简体中文与 English，架构可扩展更多语言） |
| 音频 | 自有 Rust 引擎（Symphonia 解码 + CPAL 输出）；浏览器预览走 MockEngine |
| 歌词 | AMLL（Apple Music-like Lyrics）逐字渲染 |

## 环境要求

- Node ≥ 20、pnpm ≥ 10
- Rust stable ≥ 1.85（`shannon-audio` 的 Symphonia 0.6 要求；其余 crate 只需 1.77）、`cargo tauri`（`cargo install tauri-cli` 或使用 `pnpm tauri`）
- macOS：Xcode Command Line Tools

## 常用命令

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 启动 Tauri（自动拉起 Vite dev server 于 :1420 并打开窗口）
pnpm dev              # 仅前端，浏览器预览 http://localhost:1420
pnpm build            # 前端类型检查（tsc）+ 产物构建（vite）
pnpm tauri build      # 打包桌面应用
```

```bash
cargo test -p shannon-core                                # 曲库扫描与稳定 ID；同时重新导出 ts-rs 契约类型
cargo test -p shannon-audio                               # 播放引擎；无头运行，语料现生成不入库
cargo run -p shannon-core --example scan_dump -- <目录>   # 扫描目录并打印每首曲目的规格与字段来源
cargo run -p shannon-core --example warm_cache -- <目录> <缓存路径>  # 预热扫描缓存，用于验证「重启免重扫」
cargo run -p shannon-audio --example play -- <文件>       # 试放一个文件，打印规格、协商结果、位置与欠载
cargo run -p shannon-audio --example devices              # 列出输出设备支持的声道数、采样率与采样格式
cargo run --release -p shannon-audio --example bench_decode -- <目录> [--jobs N] [--loudness]  # 解码/响度分析吞吐基准
cargo run -p shannon-audio --example make_corpus          # 用 ffmpeg 生成格式矩阵测试语料（需 ffmpeg）
cargo run -p shannon-audio --example make_playlist -- <源音频>  # 生成多格式实测歌单（给耳朵听的）
cargo run -p shannon-audio --example play -- audio/playlist   # 连续播放整个歌单
```

格式矩阵测试（`audio/tests/format_matrix.rs`）验证每种启用的编码真的解得对、放得完，
其语料由上面的 `make_corpus` 生成到 `audio/tests/corpus/`（**不入库**，可复现）。
语料不在时这批用例整体跳过并打印生成命令——CI 未必装了 ffmpeg，而纯 PCM 路径
已由 `playback.rs` 里现造的语料覆盖。**新启用一个解码器 feature 就要同时把它加进矩阵**：
开 flag 是一行的事，但那一行是一句承诺。

## 目录结构

```
src/                    前端 React 应用
  components/            组件（layout / library / player / window / common）
  store/                 Zustand（player.ts 播放器领域状态、ui.ts 界面状态）
  types/                 播放器领域类型（Track / QueueItem / Lyrics / AudioDevice …）
  i18n/                  国际化（messages.ts 消息目录、index.ts 运行时）
  data/                  曲库种子数据（浏览器预览与尚未扫描时的回落，真实曲库由扫描接管）
  hooks/                 useApplyTheme（主题）、useElasticScroll（原生滚动 + 自绘滚动条）
  index.css              Tailwind 入口 + 设计 Token
core/                    曲库（crate shannon-core）：扫描、音频规格探测、稳定 ID、元数据覆盖层
audio/                   播放引擎（crate shannon-audio）：解码、PCM 管线、输出后端
src-tauri/               Tauri Rust 外壳（窗口、权限、命令与事件桥）
docs/                    开发文档
```

`core/` 与 `audio/` 都**不依赖 Tauri 与任何 GUI 库**，因此能在无图形、无声卡的环境
`cargo test`；副作用（扫描进度、播放事件）经回调注入，由外壳决定落地方式。
业务逻辑不写在 `src-tauri/`。

## 约定

- **设计 Token**：颜色一律经语义化 CSS 变量引用（`--bg`/`--tx`/`--ac` …），禁止在组件里写字面色值；深色整套替换，见 `src/index.css`。
- **国际化**：所有界面文案必须加入 `src/i18n/messages.ts` 的 `Messages` 接口并经 `useT()` 渲染，禁止硬编码；专辑、歌手等**内容**不进 i18n。漏键会在编译期报错。
- **状态**：播放器领域状态（队列、进度、歌词时间轴、音频设备）走 `src/types/player.ts` 的强类型模型，不用散装字段。

## 音频后端架构与研究

播放引擎**已接入主程序**：`audio/`（crate `shannon-audio`）打通了阶段 0 与阶段 1 的
立体声路径——ALAC / AAC / FLAC / MP3 / WAV / AIFF / CAF / Vorbis 经 Symphonia 解码 →
响度增益 → 重采样 → 声道适配 → 无锁环形缓冲 → CPAL 共享输出，含播放 / 暂停 / seek /
音量斜坡 / 响度归一化 / 无缝换曲 / 输出端点切换。界面上的播放条走的是引擎按输出回调
消费帧数上报的真实位置。多声道不走这条路：下混与空间化都交给系统（应用自行下混会把
本可被空间化的流提前拍扁），而平台原生输出后端尚未接入，当前遇到多声道报明确的路由
错误。目标分层、实时播放零子进程约束、解码后端优先级与链接导入边界
见 [音频后端架构约束](AUDIO_BACKEND_ARCHITECTURE.md)；工程结构、线程模型、队列交接、
测试策略与实施阶段见 [音频后端实现设计](AUDIO_BACKEND_IMPLEMENTATION_PLAN.md)。

已有四份概念验证记录，作为后续领域建模、能力探测与验收指标的依据，**不代表当前版本
已经提供对应播放能力**：

- [Dolby Atmos/JOC 系统解码观测](ATMOS_DECODING_NOTES.md)：比较普通 PCM 与
  `MFAudioFormat_Float_SpatialObjects` 解码结果，确认系统解码层能够暴露超过 6 路的
  spatial/object buffer。
- [AC-4 内部空间元数据兼容性](AC4_INTERNAL_METADATA_NOTES.md)：记录 AC-4 MFT 的对象坐标与
  节目响度命令、endpoint writer 长度冲突、失败链和能力探测边界。
- [固定多声道空间回放研究](WINDOWS_SPATIAL_PLAYBACK_NOTES.md)：整理 7.1.4、9.1.6、22.2
  的容器识别、布局解析、流式解码、seek、`ISpatialAudioClient` 路由与无听感验证方法。
- [macOS 空间音频回放观测](MACOS_SPATIAL_PLAYBACK_NOTES.md)：AVPlayer 直通与 PCM 经应用
  中转两条路径的对比。两者的**听感尚未比对**，结论未定之前不得据此描述能力。

前三份（Windows 侧）涉及的三类问题必须分开建模：E-AC-3/JOC 笔记关注系统解码输出，AC-4 笔记关注解码器到 endpoint 的
私有 metadata 兼容边界，固定多声道笔记关注把既有 PCM 布局放入静态类型或固定坐标槽位。借用
动态对象 API 的固定坐标槽位不表示输入文件含有动态对象元数据。

## 设计来源

Claude Design 项目「香农播放器设计简报」，共 10 个页面 + 设计 Token 文档，定稿方向
「杏色 · 明快 2a」。离线参考包已整理在 [docs/design](design/README.md)；设计稿页面已全部
落地（曲库 / 专辑 / 歌手 / 歌曲 / 歌单 / 收藏 / 搜索 / 设置 / 歌词 / 首启引导），新页面
应复用现有 Token、i18n 与 store 体系。
