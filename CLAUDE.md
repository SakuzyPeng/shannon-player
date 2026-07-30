# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

香农播放器（Shannon Player）：Tauri 2 + React 19 桌面本地音乐播放器，AGPL-3.0-only。文档与代码注释以简体中文为主。

## 常用命令

```bash
pnpm install          # 安装前端依赖（包管理器固定用 pnpm）
pnpm tauri dev        # 启动桌面应用（自动拉起 Vite dev server 于 :1420）
pnpm dev              # 仅前端，浏览器访问 http://localhost:1420
pnpm build            # 类型检查（tsc --noEmit）+ 前端产物构建（vite）
pnpm tauri build      # 打包桌面应用
```

```bash
cargo test -p shannon-core                                # 后端单测；同时把 ts-rs 契约类型重新导出到 src/types/generated/
cargo test -p shannon-core id_survives_rename             # 跑单个测试（标准 cargo 名字过滤）
cargo run -p shannon-core --example scan_dump -- <目录>   # 扫描目录并打印每首曲目的完整规格与字段来源
cargo run -p shannon-core --example warm_cache -- <目录> <缓存路径>  # 预热扫描缓存，用于验证「重启免重扫」

cargo test -p shannon-audio                               # 播放引擎测试（无头，语料现生成不入库）
cargo run -p shannon-audio --example play -- <文件>       # 试放一个文件，打印规格、协商结果、位置与欠载
cargo run -p shannon-audio --example devices              # 列出输出设备支持的声道数/采样率/采样格式
cargo run -p shannon-audio --example make_corpus          # 用 ffmpeg 生成格式矩阵测试语料
cargo run -p shannon-audio --example make_playlist -- <源音频>  # 生成多格式实测歌单（给耳朵听的）
cargo run --release -p shannon-audio --example bench_decode -- <目录> [--jobs N] [--loudness]  # 解码/完整响度分析吞吐基准
```

`pnpm build` 会被 pnpm 的 minimumReleaseAge 策略挡在依赖检查这一步（Radix 的新版本刚发布不久）。绕过办法是直接调本地二进制：`./node_modules/.bin/tsc --noEmit` 与 `./node_modules/.bin/vite build`，校验效果一致。

前端没有测试套件，`pnpm build` 是主要的正确性校验（i18n 漏键、ts-rs 契约漂移、类型错误都在这里暴露）；Rust 侧的正确性校验是 `cargo test -p shannon-core`（扫描聚合、稳定 ID 都有单测，且不需要图形环境）与 `cargo test -p shannon-audio`（解码、环形缓冲、重采样、端到端播放，同样无需图形与声卡）。

**启用一个解码器 feature 就要同时把它加进 `audio/tests/format_matrix.rs` 的矩阵**：开 flag 是一行的事，但那一行是一句承诺，没有语料覆盖的承诺等同于赌它能用（验收条件第 7 条「未经证实的状态不得展示」）。矩阵语料由 `make_corpus` 用 ffmpeg 从同一份扫频源转出（**不入库**，可复现），验证规格读取、**无损格式与源逐样本一致**、有损格式能量守恒、端到端播完零欠载，外加一条混合格式「歌单」用例覆盖换曲时的整套重建。语料不在时这批用例跳过并打印生成命令——CI 未必有 ffmpeg，PCM 路径已由 `playback.rs` 现造的语料兜住。别用 `afconvert` 之类的平台工具造语料，那会把验证能力绑死在一台机器上。

**自动化断言证明不了「听着对」**，所以另有一份给耳朵的歌单：`make_playlist` 把一首真实音乐转成 11 种格式（无损五种应当听不出差别；44.1k 与 48k 两份无损的对比就是重采样质量；有损那几首听开头结尾有无咔哒），`play` 传目录即连续播放。它与 `make_corpus` 的分工是「给耳朵」与「给断言」，两者都不入库。

验证 UI 效果用 Playwright MCP 指向 `http://localhost:1420` 截图——Tauri 原生窗口截图受 macOS 屏幕录制权限限制，而 Vite dev server 渲染的是同一份前端。注意 Radix 菜单需要真实 pointer 事件，合成 `.click()` 不会触发，需用 browser_click。

**在浏览器预览里用真实曲库**（种子数据没有真实封面、长标题、合辑这些真实形态）：先 `cargo run -p shannon-core --example warm_cache -- <音乐目录> "$HOME/Library/Application Support/com.shannon.player/library-cache.json"` 产出缓存、封面与预览快照，再在页面里注入——dev 构建把 store 挂在了 `window.__shannon`（见 `src/main.tsx`），Vite 的 `server.fs.allow` 也已放行该目录：

```js
const base = "/@fs/Users/<你>/Library/Application Support/com.shannon.player";
const snap = await (await fetch(base + "/library-snapshot.json")).json();
window.__shannon.library.getState().setCoverDir(base + "/covers");
window.__shannon.library.getState().setLibrary(snap);
```

`window.__shannon.ui.getState().setView("list")` 这类调用还能绕开被拖拽区遮挡、点不到的控件。

## 架构

三层结构：前端 + Tauri 薄壳 + 纯逻辑 Rust core，cargo workspace（`Cargo.toml` 成员 `core`、`src-tauri`）与 pnpm scripts 串联。

- **`src/`** —— React 19 + TypeScript + Vite 前端，承载全部 UI 与交互逻辑。
- **`src-tauri/`** —— Tauri 外壳，只做四件事：注册命令（扫描 / 取曲库 / 取音乐文件夹 / 取封面目录 / 元数据改写与还原 / 播放控制）、把 core 与 audio 的回调转成 Tauri event（`library://scan-progress`、`player://event`）、用 `LibraryState` 持有扫描缓存与覆盖层、把状态落到应用数据目录（`library-cache.json` / `metadata-overrides.json` / `playback-session.json` / `loudness-analysis.json` / `ui-settings.json`，均为原子写；封面缩略图在同目录的 `covers/`；SQLite 尚未引入）。封面经 asset 协议加载，因此 `tauri.conf.json` 开了 `assetProtocol`（scope 限定 `$APPDATA/covers/**`）且 `Cargo.toml` 需带 `protocol-asset` feature。此外负责窗口：macOS 用系统标题栏（`titleBarStyle: "Overlay"`），Windows / Linux 无边框（`decorations: false`）＋透明＋自绘交通灯（`src/components/window/TrafficLights.tsx` 经 `@tauri-apps/api/window` 调原生窗口控制，权限声明在 `src-tauri/capabilities/default.json`），详见下文「窗口外观按平台分两套」。**业务逻辑不写在这里。** 三份落盘数据的重要性不同，处理方式也不同：缓存可重建（损坏就重扫）；**覆盖层不可重建**（用户手改的元数据，损坏时保留 `.corrupt` 残骸而非静默覆盖）；播放会话可重建但用户会在意（丢了重新点一次歌，可每次重启都丢很烦），读取失败一律**静默当作没有会话**——为一份能随手重建的数据弹错误框，打扰的成本高于它本身的价值；响度分析结果性质介于两者之间（可重建，但重建代价是全库解码一遍），读不懂时当空但不留 `.corrupt` 残骸——那种残骸只对用户手改过的数据有意义。
- **`core/`（crate `shannon-core`）** —— 曲库扫描、音频规格探测、稳定 ID、元数据覆盖层。**刻意不依赖 Tauri 与任何 GUI 库**，因此能在无图形环境 `cargo test`；副作用（进度上报）通过回调注入，由外壳决定落地方式。新增后端能力优先放这里，让外壳保持薄。
- **`audio/`（crate `shannon-audio`）** —— 播放引擎：解码、PCM 管线、输出后端。同样零 Tauri 依赖（gapless、seek、欠载压测因此能无头 `cargo test`）。当前状态是**阶段 0 的立体声路径**：ALAC / AAC / FLAC / MP3 / WAV / AIFF / CAF / Vorbis 经 Symphonia 解码 → 重采样 → 声道适配 → 无锁 SPSC 环形缓冲 → CPAL 共享输出，含播放 / 暂停 / seek / 音量斜坡。**多声道不走这条路**——下混与空间化都交给系统（见下），平台原生后端尚未接入，当前遇到多声道报明确的路由错误。已接进 Tauri 与前端（命令 + `player://event` 事件桥），播放条走的是引擎上报的真实位置。

管线里有三处顺序是想清楚才这么定的，改动前先看 `docs/AUDIO_BACKEND_IMPLEMENTATION_PLAN.md` 的「管线顺序」：① **音量在输出回调里做，不在管线里**——管线领先播放一秒半，在那儿改增益意味着按下静音要等一秒半才生效；② **重采样在声道适配之前**（按源声道数做），通则是在声道数少的那一侧重采样，所以将来加多声道下混时顺序要反过来；③ **协商与打开流分成两步**（`OutputBackend::negotiate` 只预演不碰设备），因为环形缓冲容量、重采样比率、位置计数的时基都要按协商结果搭。另外**位置计数一律记输出域的帧**——`Decoder::seek` 返回的是源域帧位置，混用会让进度按比率走偏（44.1 → 48 kHz 快 8.8%）。

**扫描分三步，别把它们揉在一起**：`scan::scan_folders` 只产出 `ScanCache`（原始探测结果，不含封面字节只留指纹）→ `ScanCache::library(&Overrides)` 套用用户覆盖并聚合成 `LibrarySnapshot`（纯内存，毫秒级）→ 外壳把两者分别落盘。分开的理由是「改一次元数据不该重扫整库，重启也不该」：归组依据（原始标签、封面指纹、路径）在聚合后的快照里已经丢失，只留快照就只能回头重读文件。

### 关键机制（跨文件才能看清的部分）

**设计 Token 与主题切换**：`src/index.css` 中 `:root` 定义浅色 CSS 变量（`--bg`/`--tx`/`--ac` 等），`[data-theme="dark"]` 整套覆盖；Tailwind v4 的 `@theme inline` 把它们映射为 `--color-*`，因此 `bg-bg`、`text-tx` 等工具类会随 `<html data-theme>` 实时换肤。`src/hooks/useApplyTheme.ts` 负责写 `data-theme` 并监听系统偏好（"system" 模式）。**组件里禁止写字面色值**，一律走语义变量。

**状态**：Zustand 两个 store——`src/store/player.ts`（播放器领域：队列/进度/音频设备/循环，类型全部来自 `src/types/player.ts` 的强类型领域模型，不用散装字段）、`src/store/ui.ts`（主题/视图/导航/语言）。

**国际化（第一天就做，强类型）**：`src/i18n/messages.ts` 的 `Messages` 接口是全部文案键的单一来源，任一语言字典漏键会编译报错。组件内经 `useT()` 取翻译，支持 `{var}` 插值。新增任何界面文案必须加 key，禁止硬编码；专辑名、歌手名等**内容不翻译**。

**语言范围承诺（重要约束）**：对外只承诺简体中文 + English。`src/data/library.ts` 的 `LANGUAGES` 与 `src/i18n/index.ts` 的 `detectSystemLocale()` 只暴露这两者；zh-Hant / ja 的词条在 `messages.ts` 中备好但**不得**加入 UI 菜单或系统语言解析，文档中也不得宣传，除非用户明确解除该限制。

**前后端契约（ts-rs，与 i18n 同一理念）**：`core/src/model.rs` 的结构体带 `#[ts(export, export_to = "../../src/types/generated/*.ts")]`，`cargo test -p shannon-core` 会跑 `export_bindings_*` 测试把 TS 类型写进 `src/types/generated/`（`audio.ts` / `library.ts`）。生成物**入库**（前端无 Rust 工具链也能编译），但**禁止手改**——改 Rust 结构后重跑 cargo test 再提交，Rust 一漂移前端 `pnpm build` 就报错。序列化统一 `#[serde(rename_all = "camelCase")]`。

**曲库数据流（seed / scan 双源）**：`src/lib/backend.ts` 是**唯一** IPC 出入口，组件不直接 `invoke`——浏览器 dev 环境（无 Tauri）只在这一处回落，调用点不写环境判断。数据落到 `src/store/library.ts`，`source` 字段区分 `seed`（`src/data/library.ts` 的设计稿种子曲库，浏览器预览或尚未扫描时用，保留它是为了界面不空、UI 开发能继续）与 `scan`（真实扫描）；整库替换时 `version + 1`，`App` 以它为 key 强制重挂载，避免各页缓存旧曲库的派生结果。启动时 `App` 的 `useRestoreLibrary` 从后端缓存恢复曲库（不重扫）。组件读曲库一律经 `src/lib/library.ts` 的访问器（`albums()` / `tracksOf()` / `topTracksOf()` …），不直接碰种子数据；真实曲库没有的派生数据（如热门歌曲的播放统计）要显式退化，不假装有。`src/data/playlists.ts` 是**纯数据模块，只能用种子曲库**——改成从 store 取「生效曲库」会与 `store/player.ts` 形成循环依赖（后者要 import `PLAYLISTS`）。

**音频规格建模戒律（`core/src/model.rs` + `core/src/probe/`）**：① 声道**位掩码是权威**，具名 `ChannelLayout` 只是它的投影——6 声道可能是 5.1 也可能是 6.0，摆位不同下混系数就不同，**判不出一律留空，不用声道数硬猜**；② 空间音频（`SpatialFormat`）与声道维度**正交独立**，Atmos 的声道数可能报 5.1 甚至 2；③ `codec` / `container` 存探测器报告的原始名，不归一化（归一化会丢信息）；④ **识别与播放能力解耦**——扫描只如实记录规格，播不了是播放器的事，不能因为暂时播不了就在扫描阶段丢文件；解析失败计入 `failed` 上报，不静默丢弃；⑤ 增强探测逻辑时必须把 `PROBE_VERSION` +1，否则无法识别哪些条目需要重扫，读不懂的线索塞 `probe_notes` 留待回溯。

**稳定曲目 ID（`core/src/id.rs`，改动前先读该文件顶部的取舍说明）**：收藏、歌单、**元数据覆盖**都以曲目 ID 为键，所以 ID 必须扛得住文件被移动、重命名、改标签——方案是「文件大小 + 跳过元数据区后的三段内容 blake3 + 格式指纹」。**改采样点或指纹构成 = 全库 ID 变化 = 用户的收藏、歌单与元数据修改全部失联**，除非同时给出迁移方案，否则不要动。专辑 ID 相反，它是聚合派生的（改专辑艺人就变），**只能用于会话内导航，绝不能当持久化的键**。

**专辑聚合（`core/src/scan.rs` 的 `aggregate`，两遍 + 一次合并）**：专辑艺人是**组级**结论，单看一首歌无法判断它属于某位歌手的专辑还是一张合辑——逐曲回落到曲目艺人正是合辑曾被拆成十几张的根因。所以第一遍只定字段与来源并套用覆盖，第二遍按组决定专辑艺人（有标签用标签 → 组内多数决 ≥60% → 否则判为 `Various Artists` 合辑）。归组作用域按「用户指定的专辑艺人 > 标签专辑艺人 > 所在目录」保守选取，再按**封面指纹**合并同名专辑。封面这条规矩是踩坑换来的：**只作合并证据，不作拆分依据**——同一张专辑内部可能有几首嵌了不同版本封面，当归组键会把好好的专辑劈成两半；反过来，同名专辑之间有共同封面几乎可以断定是同一张（实测一张 116 首、横跨 6 个艺人目录的合辑，只有主创目录写了专辑艺人，唯有封面认得出）。

**封面（`core/src/cover.rs` + `src/components/common/CoverArt.tsx`）**：实测约四成封面不是正方形（1300×910、710×1000 这类），而封面卡是正方形，因此**后端一次性合成**「原图完整居中 + 同图放大模糊填充四周」的正方形 JPEG。合成放后端而不是前端用两层 DOM 加 CSS 模糊，是因为专辑网格会同时显示几十张封面，每张挂一个模糊图层就要每帧重算（实测后端合成方案滚动零掉帧，中位帧 8.3ms）。缩放规矩：**只缩小不放大**（原图比档位小就按原尺寸存，放大交给 GPU）；缩小用 Lanczos3 且**逐档接力**（原图 → 1024 → 512 → 128，避免大比例缩放时滤波核开销爆炸）；档位按界面实际显示尺寸 × 2 倍屏推算，前端 `src/lib/cover.ts` 的 `pickSize` 按显示边长挑档。封面按**内容指纹**去重（939 首实测只对应 28 张唯一封面），文件名即指纹，重扫时已存在就跳过解码。占位渐变**始终生成**，图未到位、无内嵌封面、文件损坏三种情况自动回落，调用点不写错误分支。

**叠在封面卡上的元素必须同时继承 `border-radius` 与 `corner-shape`**：`.cover-corners` 用 `corner-shape: superellipse(1.5)` 做连续圆角，而 `corner-shape` 不是可继承属性。只写 `border-radius: inherit` 的话，子元素是标准圆角、容器是 superellipse，子元素在四角比容器少盖一圈，底下的深色占位渐变就露出来——浅色封面上看着像四道黑边。写法是 `style={{ borderRadius: "inherit", cornerShape: "inherit" }}`（显式继承，圆形头像那种没有 `.cover-corners` 的容器会继承到默认 `round`，不会被误变成方角）。宽高比差 2% 以内直接当正方形处理（这些参数与「中位色打底」都参考了同一作者的 Swift 项目 `LGP3` 的 `CoverImageProcessor`）。

**重复曲目与多碟（`core/src/scan.rs`）**：真实曲库里同一首歌常有多份拷贝（导入工具留下的 `xxx 1.m4a`、整盘版与分碟版并存），实测一个库 939 个文件里 608 首是副本。这些副本音频相同但元数据差几十字节，**文件大小不同所以曲目 ID（内容哈希）也不同**，靠 ID 去不掉——三条判据任一命中即为同一首：「精确标题 + 时长（0.01 秒）」、「去掉末尾译名括注后的标题 + 时长」或「碟号 + 音轨号 + 时长」。**三条都额外要求音频规格一致**（编码 / 采样率 / 位深 / 声道数），否则同一首歌的不同**格式**版本会被当副本吃掉——实测把一段音乐转成 11 种编码放进同一目录，扫出来只剩 4 首。分界在于用户知不知情：导入工具留下的副本是字节级拷贝、规格必然相同，而同时留着 FLAC 与 MP3 的人是有意为之，折叠等于替他做了没同意的删除。已知残留：同编码不同码率（MP3 128k 与 320k）规格键相同仍会折叠，没把码率纳入键是因为它由文件大小估算，副本间差几十字节就可能抖动 1 kbps。第二条抓 `Winter Alice` / `Winter Alice（冬日爱丽丝）` 这类标题主体相同但译名有无不一、轨位标签又互相冲突的副本；第三条抓同轨位却用了不同标题的副本。三条都**只在同一张专辑内比较**且要求时长一致，跨专辑收录与同时长不同版本不受影响。重复组的碟号与音轨号按副本多数决，随后优先保留轨位一致且信息最全的一份；折叠数经 `LibrarySnapshot.duplicates` 如实上报。多碟专辑另需在归组前剥离 `Disc N` / `CD N` 后缀（碟名常被写进专辑标签），匹配只认独立词，`Discovery` 不会误伤。

**扫描要跳过包目录**：macOS 的「包」（`.bundle` / `.logicx` / `.band` / `.app` / `.musiclibrary` …）在 Finder 里显示为单个文件，用户不认为里面有音乐。实测 `~/Music` 除 954 个音乐文件外，另有 56 个音频躺在 `Logic Pro Library.bundle` 的采样库里（单周期波形只有几毫秒），`.logicx` 工程包里还有母带片段，唯一一例解码失败也出自其中——收进曲库与把 `.app` 里的提示音收进来是同一类错误。`collect_files` 用 walkdir 的 `filter_entry` 在**进入前**剪枝（包内成千上万的素材根本不会被 stat），候选数由 1010 降至 944。两处细节：① `depth() == 0` 例外，**用户显式指定为扫描根的包照扫**——想把采样包当曲库是他的自由，否则设置页加了目录却扫出 0 首无从解释；② 列表只收**目录**形态的扩展名，`.dmg` / `.als` / `.ptx` 是单个文件，列进去不生效只会误导。

**标签读不到 ≠ 不是音频**：`lofty` 读不了的合法容器确实存在（实测 CAF），早先那里是硬失败，一个合法文件只体现为 `failed` 计数 +1，用户看到的是「文件明明在，曲库里找不到」。现在降级处理——标签留空、规格与时长改由 symphonia 顶上、`probe_notes` 记 `tags:unreadable`，只有**标签与规格双双读不出**才判定不是音频（垃圾文件仍会被拒）。这与「识别与播放能力解耦」是同一条：信息少不等于不存在。

**缺失值的表达**：判不出的字段一律留空而不是填哨兵值——`Album.year` 是 `Option`，因为填 0 会一路漏到界面上显示成「0 年」。界面拼接元信息用 `src/lib/meta.ts` 的 `metaJoin`，它会跳过缺失项，避免出现「白鲸电台 · 」这种孤零零的分隔符。多碟专辑的曲目列表按碟分节、组内用真实音轨号（`AlbumDetailScreen` 的 `discs`）：序号若用列表索引，两张碟拼起来第二碟第一首就会显示成 16。**音轨号缺失时显示 `·` 而不是退回序位**——序位是个会撞号的哨兵值，同一张专辑里只要有几首缺标签，编出来的号就会与其它曲目的真实音轨号重号（实测一张 11 首的专辑同时出现两个「10」和两个「11」）。

**元数据覆盖层（`core/src/overrides.rs`）**：只要存在兜底推断就会猜错，就必须让用户能改。三条规矩：① 键用曲目 ID，专辑级编辑写入时展开成逐曲记录；② 只存被改过的字段，`None` = 不覆盖，这样重扫读到更好的标签时用户没碰过的字段仍会更新；③ 字段三态——没动 / 显式清空（文本空串、数字 `null`）撤销该字段 / 改值，只有两态用户就只能整条还原。三态靠**补丁与落盘分开建模**实现：落盘的 `TrackOverride` 只需两态，请求用的 `TrackMetadataPatch` 才要三态，其中数字用 `Option<Option<u16>>` 经 serde 区分「字段缺席」与「显式 null」——数字没有空字符串可借，只用一层 Option 会让碟号、音轨号只能改不能撤销。**不写回音频文件**：写标签是破坏性操作，不该是「编辑信息」的副作用。前端对应 `src/components/common/EditMetadataDialog.tsx`（含 `useMetadataEditor` hook，各页菜单接入用它），界面同样**只提交用户真正动过的输入框**，否则等于把「猜的」固化成「用户指定的」。

**播放引擎接进前端的三处要点（`audio/src/contract.rs` + `src-tauri/src/player.rs` + `src/store/player.ts`）**：① **契约类型放产生它的那一层**——`PlayerEvent` 定义在 audio crate 并经 ts-rs 导出到 `src/types/generated/player.ts`，与 `ScanProgress` 放在 core 同理；放进外壳会让本该薄的它承担领域建模，而那个定义再也无法被无头测试覆盖。注意 `#[serde(tag = "...")]` 的 `rename_all` **只作用于 variant 名**，字段名要另外写 `rename_all_fields`，否则序列化出 `track_id` 而前端按 `trackId` 读，每个字段都是 `undefined` 且不报任何错。② **每个事件都盖 `trackId` + `loadId` 的章**：引擎只认文件路径，前端队列以曲目 ID 为键；曲目 ID 用来关联队列项，装载 ID 用来区分同一首的连续重载。章必须随 `Load` 命令进入引擎并由产生事件的代际原样回带，不能由外壳读取一个共享“最新 ID”临时盖章——命令异步入队时后一首会先覆盖它，反而把前一首的迟到事件盖错。③ **引擎懒起**：`Engine::spawn` 到首次 `load` 才打开输出设备，在 `setup` 里就建好等于应用一启动就占声卡。

**播放状态有两个，别用一个推另一个**：`loadedTrackId`（引擎装载的是哪首）与 `current()`（播放条显示的是哪首）是两件事。曲库恢复后队列换上了真实曲目而引擎那边还是空的，此时按播放，`engine.play()` 打在空引擎上是 no-op，界面却已乐观地把图标切成暂停——实测就是按钮变了、进度一动不动。同源的一条：**曲库到位后要把种子演示队列换掉**（`adoptLibrary`），否则一个已经扫描过的用户开门见到的仍是演示曲目，按下播放得到「这是演示曲目」——他明明有音乐，播放器却说他没有。换的时候**整库入队**而不是只放第一首，只放一首会让「下一首」原地打转，按钮亮着却什么都不发生。

**随机播放要洗牌，不要每次随机取一首**：后者会重复、会漏，一个 10 首的队列放到第 10 首时仍有约 35% 的曲目一次没放过。`shuffleOrder` 存的是一次洗好的 uid 排列，且**把当前曲目固定在首位**——用户按的是「之后随机」，不是「立刻换一首」。队列增删要同步维护它，否则轮到一个已被移除的项时会直接停住。

**播放失败不自动跳下一首**：整库格式不支持时那会变成一场无声的快进，用户完全不知道发生了什么。失败要停下并按 `kind` 分类说明（找不到文件 / 格式不支持 / 设备被占用，用户要做的事完全不同），见 `src/components/player/PlaybackNotice.tsx`。

**响度归一化的四条边界（`audio/src/loudness/` + `src-tauri/src/loudness.rs` + `src/hooks/useLoudnessQueue.ts`）**：① **顺序由前端给，增益由后端算**——优先级是「距当前播放位置的远近」，而队列的权威在前端（随机顺序、循环、拖拽重排都在那边），后端自己排就要复制一份队列状态；反过来增益取决于分析结果与播放策略（目标 -18 LUFS、上限 -1 dBTP），改策略不该要求前端跟着改，前端只传「用户要不要归一化」这一个布尔。② **增益施加在管线里（解码后、重采样前），与音量恰好相反**——做 gapless 后环形缓冲会同时躺着两首歌的 PCM，输出回调里那个「当前增益」必然在边界处把前一首的尾巴用后一首的增益放出去；放在重采样之前还让 flush 冲出的尾部延迟也是归一化过的。异常增益（非有限、负数、0）一律回落 1.0 而不是钳到 0：算歪的增益让整首歌没声音，比不归一化糟糕得多。③ **确定结论与瞬态错误分开**——测不出（全静音、极短）与暂不支持（多声道）可缓存；文件读不到、解码失败什么都不记，下次重排队列时重试，否则一次网络盘掉线就让那首歌再也不会被分析。④ **两个版本号管不同的事**——文件上的 `schemaVersion` 不认识就整份丢弃，每条记录上的 `analysisVersion` 覆盖测量器版本 / feature / Mode / 真峰值算法 / 声道映射，不符视为 miss 并逐条重测；合成一个的话测量器一升级就得连带丢掉整份文件。结果只存原始事实，**不存最终增益**（改策略应能立即重算）。并发度固定 1 个后台 QoS worker：真正知道此刻该给后台任务多少资源的是操作系统调度器，所以既不做成设置项，也不在用户机器上跑 benchmark。

**播放会话持久化（`src/lib/session.ts` + `src-tauri/src/frontend_state.rs`）**：队列、当前曲目、播放位置、循环与随机状态跨重启保留。四条规矩：① **只存曲目 ID，不存 `Track` 对象**——曲目信息的权威在曲库，存副本会让用户改完元数据重启后队列里还是旧标题（他刚改过，这种不一致最让人怀疑软件坏了）；顺带省下整库入队时抄一遍曲库的体积，且文件删除 / 重扫后 ID 变化时自然剔除。代价是**恢复必须等曲库就绪之后**，因为要按 ID 回查。② 随机顺序存的是**下标排列**而非曲目 ID——队列允许同一首歌多次入队，用 ID 表达顺序会有歧义。③ 后端只负责原子存取一段文本，schema 与版本号由前端拥有：会话是**前端拥有**的状态，后端在其中没有领域判断可做，给它定义 Rust 结构只会让每加一个字段都要改两处、重跑契约导出。④ 待恢复位置绑定**队列项 uid**（不能只绑曲目 ID，同一曲目可重复入队），并随有效音量一起放进同一条 `Load` 命令；引擎先定位、建立输出域位置基准并预缓冲，最后才按 `autoplay` 解除暂停。等 `Opened` 跨 IPC 回来再补 `Seek` 会先漏出一段曲首 PCM。

**界面设置持久化（`src/lib/settings.ts` + `src/main.tsx`）**：主题、视图、语言与设置页开关跨重启保留，与播放会话共用 `frontend_state.rs` 那条「后端只原子存取一段文本」的通道（槽位是**固定常量**，不接受前端传来的文件名——那等于把应用数据目录的任意路径交给渲染进程）。三条：① **原生窗口在首屏准备好之前保持隐藏**（两份 Tauri 配置都设 `visible: false`）：只把读取放在 `createRoot` 前还不够，等待 IPC 时 CSS 已经可能画出默认浅色背景；正确顺序是读设置 → 灌入 store → 同步写 `<html data-theme>` → `flushSync` 提交 React 首屏 → `show()`，外壳三秒后兜底显示，避免前端异常时窗口永久不可见。② 因此这里**不需要** `sessionReady` 那样的就绪守卫：落盘的值在首帧之前就进了 store，任何时刻写出去的都已经是用户的设置而不是默认值（StrictMode 的 cleanup 补写因此无害）。③ 每个字段按**当前支持范围**校验，未知值回落默认：语言只认 `LANGUAGES` 那三项——`Language` 类型里还有繁體中文与日本語（词条备好但没进 UI），照单全收就能让一份旧文件或手改把界面切到我们并未对外承诺的语言上；开关按键逐个取并与默认合并，整份替换会让将来新增的开关在旧文件下变成 `undefined`，界面上显示成「既不是开也不是关」。

**「卸载时保存」在 StrictMode 下会变成「启动时保存未初始化状态」**：React StrictMode 开发模式下 mount → unmount → mount，于是 `usePersistSession` cleanup 里那句「退出前补存一次」在**应用刚启动**时就执行了，把种子演示队列写进了会话文件，覆盖掉用户上次的真实会话；而种子曲目的 ID 在真实曲库里查不到，下次恢复整个失败，表现为「队列每次重启都回到第一首」。实测复现。挡住它的是 store 里的 `sessionReady` 守卫——**恢复流程跑完之前一个字都不写**。首次扫描不经过启动恢复，也必须在真实曲库接管队列后显式置位；就绪状态由两条流程各自的汇合点完成，不能藏进「也许会因用户已经操作而跳过」的 `adoptLibrary`。教训是通用的：不能指望「卸载」一定意味着「用完了」，凡是在 cleanup 里做持久化的地方都要问一遍这个。

**共享元素过渡的副作用**：`layoutId`（专辑卡 ↔ 详情页大封面的共享过渡）会**顺带开启 layout 动画**，于是同一页内任何导致该元素位移的布局变化——比如歌手页展开/收起歌曲列表——都会被逐个做成 spring 动画，低阻尼参数下还会过冲成「弹跳」。修法是 `layoutDependency={不变的值}`：位置变化不再触发重新测量，而靠 `layoutId` 配对的跨组件共享过渡照常工作。给带 `layoutId` 的元素所在区域加可展开/折叠的内容时，都要留意这一点。

**滚动**：滚动「手感」交还各平台原生（macOS 触控板橡皮筋、Windows/Linux 滚轮惯性各自沿用系统实现），只统一「视觉」。`src/hooks/useElasticScroll.ts`（名称沿用，实为「原生滚动 + 自绘滚动条」）不再拦截 wheel、不再自积分物理，仅：容器用 `.no-scrollbar` 隐藏系统滚动条，并按原生 `scroll` 事件的 scrollTop/scrollHeight 直接映射绘制一份跨平台一致的 6px thumb（静止 0.9s 后淡出，内容未溢出不显示）。返回签名 `{ scrollerRef, innerRef, thumbRef, onScroll }` 不变，`innerRef` 现仅作内容容器，但保留 `will-change:transform`——它把内容提升为一张缓存的合成层，令 superellipse 圆角 + 多重内阴影的封面卡只光栅化一次、滚动时纯合成（去掉会导致专辑网格滚动掉帧）。曾有一版自定义速度积分 + 橡皮筋引擎，因难以在各平台/输入设备上都贴合原生肌肉记忆，权衡后回退为原生手感。

**窗口外观按平台分两套（`src-tauri/tauri.macos.conf.json` 覆盖主配置）**：

- **macOS 用系统的**：`decorations: true` + `titleBarStyle: "Overlay"` + `hiddenTitle` + `trafficLightPosition`。曾经三平台统一自绘交通灯（设计稿画的就是 macOS 那三颗），但**绿灯远不止「最大化」**——hover 会展开窗口平铺面板（移动与调整大小 / 填充与排列 / 全屏幕），其中「排列」要摆布**其他应用**的窗口，是系统私有能力，自绘再怎么仿也拿不到。改走系统标题栏后，窗口圆角、投影、双击标题栏缩放、边缘拖拽、平铺菜单全部回归原生，前端 `TrafficLights.tsx` 在 macOS 只渲染一块等大占位（系统按钮浮在内容上不占布局，不留空「香」字会被压在按钮底下）。`trafficLightPosition` 的数字**只在 macOS 27 上实测过**（按钮直径约 14、中心距 23，整组 62px，按 Big Sur 的 52px 去算会偏右 4px），Tauri 自己也提醒标题栏高度随系统版本变，换版本要用辅助功能 API 重新量：`osascript -e 'tell application "System Events" to tell process "shannon-player" to get {position, size} of every button of window 1'`。
- **Windows / Linux 继续自绘**（外观本就仿 macOS，视觉不分叉），于是丢了系统圆角，得自己画：`transparent: true`（圆角外要真透明）＋ `index.css` 把背景从 `body` 挪到 `#root` 再切圆角（`body` 的背景会铺满整个矩形视口，把圆角盖回去）＋ `src/hooks/useWindowChrome.ts` 打 `data-window-chrome="custom"`（浏览器预览与 macOS 都不打）与 `data-window-fit="full"`（Windows 11 的最大化窗口是直角，全屏同理）。

**系统画的 UI 不吃 `src/i18n`**：绿灯那个平铺菜单是 AppKit 提供的，它按应用声明支持的语言挑文案，没声明就退回英文——中文系统上照样弹一份英文菜单。修法是 `src-tauri/Info.plist` 里声明 `CFBundleLocalizations`（zh-Hans + en，与「只承诺简中和 English」一致）。**只在打包成 .app 后生效**：`tauri dev` 跑的是裸二进制，没有 bundle 就没有 Info.plist 可读，dev 下永远是英文，别当成没修好。验证办法是读交通灯按钮的辅助功能描述（`get description of every button of window 1`），中文显示「关闭按钮」，英文显示 `close button`。**验证只能靠 `screencapture` 截原生窗口再裁角放大**——Playwright 看到的是浏览器画的窗口，圆角在那儿根本不存在。

**窗口自适应（布局戒律）**：窗口可任意拉伸，需保证的区间是 [980×640, ∞)（下限由 `src-tauri/tauri.conf.json` 的 minWidth/minHeight 兜底，设计稿画板 1180×760 只是默认尺寸）。规则：卡片网格一律 `repeat(auto-fill, minmax(…, 1fr))`，禁止固定列数（`grid-cols-4` 这类会在超宽下把封面撑到失衡）；文本容器一律 `truncate` + `min-w-0`；滚动区一律 `absolute inset-0 overflow-auto` 套 `min-h-0 flex-1`（高度变矮只增加滚动，不裁内容）；长文页面用 `max-w` 居中；对高度敏感的固定尺寸元素（如歌词页封面）用 `min(设计值, Nvh)` 收敛。改动布局后用 Playwright 在 980×640 与 1920×1080 各截一轮，并断言 `document.documentElement.scrollWidth <= clientWidth`（零横向溢出）。

**设计稿的隐含前提要在真实曲库上复核**：设计稿按「一张专辑一位歌手」画，专辑详情页的曲目行因此只有标题；但真实曲库里合辑、致敬盘、社团专辑占了相当比例（实测 28 张有 10 张的曲目艺人不止一位），这类列表必须按需补出歌手列。同类判断一律**看数据而不是看类型**——用「实际存在几个不同的曲目艺人」决定，而不是只信 `compilation` 标记。

**歌手不等于专辑艺人**：`albumsOfArtist` 只匹配专辑艺人，而合辑与致敬盘里的演唱者多半从没当过任何专辑的专辑艺人（实测一张致敬盘的 10 位演唱者无一有页面）。凡是「按歌手取内容」的地方都要想清楚要的是哪一种：歌手**列表**页用专辑艺人（只列主要艺人）没问题，歌手**详情**页必须用 `albumsRelatedToArtist` / `tracksByArtist`（参与过的专辑 + 演唱过的曲目），否则从合辑点进来就是一片空白。另外详情页在数据为空时 `return null` 会连返回按钮一起吞掉，用户只能靠侧边导航逃出去——空态要么给内容，要么至少留下退路，专辑 / 歌手 / 歌单三处统一走 `src/components/common/DetailNotFound.tsx`（新增详情页照此办理）。这条不是假想：专辑 ID 是聚合派生的，重扫后就会变；歌单可以在详情页开着的时候被删掉。

**折行戒律（按体裁分类，不要凭感觉）**：① 控件（按钮 / pill / tab / 菜单项）一律不换行——胶囊形状本身是可点击性的视觉暗示，被撑成两行等于换了个组件；写法是 `flex-none` + `whitespace-nowrap`。② 头部元信息副标题（`3 首 · 2 张专辑 · …` 这类一行摘要）也要规避换行：它在 `items-end` 的头部里，换行会顶高整个头部，拖动窗口时标题会跳；做法是标题列 `flex-none`（页面身份优先），并把文案交给 `src/components/common/MetaLine.tsx` 渲染——每个片段包成 `whitespace-nowrap`，极窄时只能断在 ` · ` 处，永不出现「1」与「playlists」被拆开的孤字断行。③ 正文 / 说明 / 空态本来就是多行排版，不用管。**头部空间不足时的让位顺序是固定的**：标题列与控件都不压缩，压力全部由过滤钮（`FilterPill`）吸收——收起态是固定圆钮不参与压缩，展开态可收缩至 132px 下限。

**字体覆盖范围**：`--font-serif` 里 Lora 只覆盖拉丁、Noto Serif SC 补 CJK，**两者都没有数学运算符一类的符号**（实测 `∀` U+2200 不在其中，`document.fonts.check` 直接返回 false）。缺字时浏览器回落到系统默认字体，笔画风格与 Lora 差得很明显——专辑名带这类字符时尤其扎眼。栈里因此插了一层 Times 风格的衬线数学字体，三平台各取所需（macOS 的 STIXGeneral、Windows 的 Cambria Math、Linux 的 FreeSerif / DejaVu Serif）；它们都不含 CJK，中文会继续落到后面的 Noto Serif SC。**只有 macOS 那项经过实测**，换平台开发时应重新验证——各平台自带字体差异很大，Times New Roman、Georgia、Apple Symbols 实测都没有这个字形。诊断缺字的办法：canvas `measureText` 比较「指定字体」与「必然缺失的假字体」的宽度，相同即说明该字体没有这个字形。

**i18n 单复数**：`{var|one|other}` 标记按 `params[var] === 1` 选词（实现见 `src/i18n/index.ts`）。英文凡是「数字 + 名词」都必须用，否则会出现「1 songs」；中日文无单复数变化，直接写死名词。

### 音频后端边界（动手前必读）

播放引擎已开工（`audio/`，阶段 0 的立体声路径，进度见实现计划的「阶段 0 的实现现状」），边界定死在文档里，改音频相关代码前先读 `docs/AUDIO_BACKEND_ARCHITECTURE.md`（架构约束，决策已接受）与 `docs/AUDIO_BACKEND_IMPLEMENTATION_PLAN.md`（实现级设计：工程结构、线程模型、队列交接、实施阶段）；两者冲突以架构约束为准，**要偏离就先改文档再动代码**。最硬的几条：实时播放链（播放/暂停/seek/解码/DSP/重采样/输出）必须全部在应用进程内，不得用 FFmpeg 管道、mpv 或任何**外部**播放器兜底，遇到不支持的格式就返回明确的能力错误；子进程只允许出现在非实时导入路径（用户自装的 `yt-dlp`、必要时的 FFmpeg 转封装）、开发期诊断工具，以及**自身可执行文件的隔离 worker**（仅限有明确崩溃隔离理由时，如平台 OEM 解码器在 teardown 阶段会拖垮整个进程，须共享内存通信、父进程掌握生命周期并记录隔离原因）；容器 / 编码 / 声道布局 / 输出端点能力分别建模，不许合并成一个松散的「播放格式」字段。

**多声道整体交给系统，应用不自己下混**：`mix.rs` 的职责止于立体声路径内的直通与单声道上混，引擎里没有、将来也不会有下混器。理由与「不自己双耳化」是同一条——自行下混等于把一条本可被系统空间化的多声道流提前拍扁，系统只看到两条声道，AirPods 上的空间音频开关会显示「不可用」。附带两项好处：系统掌握端点特性（耳机 / 扬声器 / HDMI 各不相同）而应用只能猜一套通用系数；以及用户在系统播放器里听到的下混结果，在这里应当是同一个。**因此 CPAL 只承担立体声，一切多声道走平台原生后端**（macOS ASBR 附 `AudioChannelLayoutTag`、Windows `ISpatialAudioClient`）——不只是空间内容，普通 5.1 也一样，因为下混与空间化都依赖布局标签而 CPAL 表达不了布局。

**空间音频是平台强相关的，别指望一套抽象盖住**：解码与输出各自按运行时能力探测选后端，同一条规则在不同平台会得出相反结论（macOS 系统解码器覆盖 APAC 与 E-AC-3/JOC，而实测 Windows 的 Opus MFT 只吃单/双声道，多声道一律拒绝，必须自己解）。两条硬约束：① **空间输出不走 CPAL**——CPAL 的配置只有声道数、表达不了布局，而布局标签正是系统判断能否空间化的依据；macOS 走 `AVSampleBufferAudioRenderer` + `AudioChannelLayoutTag`，Windows 走 `ISpatialAudioClient`。② **应用自己双耳化 = 关掉系统空间音频**——那样系统只看到一条立体声，AirPods 上的空间音频开关会显示「不可用」，头部追踪更无从谈起（渲染发生在头动之前），这是用户直接看得见的降级。另有一条 macOS 平台例外：**对象音频路径上解码器被锁定为 AVPlayer**，系统的全景声标识由上游解码链路决定而非交给渲染器的 PCM，换解码器标识就没了——这是能力有无的问题，不是质量取舍。**系统标识只能交叉印证，不能当判据**：是否为对象音频要由码流层面的 JOC 标记判定，不得因为系统显示了「杜比全景声」就在界面上跟着这么说（验收条件第 7 条）。`docs/` 下的 `ATMOS_DECODING_NOTES.md`、`AC4_INTERNAL_METADATA_NOTES.md`、`WINDOWS_SPATIAL_PLAYBACK_NOTES.md`（Windows 侧）与 `MACOS_SPATIAL_PLAYBACK_NOTES.md`（macOS 侧）是概念验证记录，是后续能力探测与验收指标的依据，**不代表当前版本已有对应播放能力**，写文档时不要把它们说成已交付功能。macOS 那份还有一条未结：两条输出路径的**听感未比对**，结论未定之前不得据此描述能力。

阶段 0 落地时踩出来的三条，改播放链路前要记得：① **单声道的判据是声道数不是掩码位置**——Symphonia 的 WAV 读取器把单声道标成 `FRONT_LEFT` 而非 `FRONT_CENTER`，按掩码比对会拒播最简单的文件；② **位置换算一律走整数**，先转秒再乘采样率会因浮点截断差一帧，进度条上看不出来，却会让「seek 后的输出等于从头解码的对应后缀」失守；③ **验证位置的测试语料必须无周期**——定频正弦每 100 帧重复一次相位，差整数个周期的偏移会被伪装成「误差为零」，上面那个 off-by-one 就是这么逃过第一轮诊断的，位置类断言一律用扫频信号。另有一条产品性质的实测：**默认输出设备不一定支持源采样率**（本机默认设备只有 24 / 48 kHz，而曲库主力是 44.1 kHz），所以重采样不是锦上添花而是可用性前提，已随阶段 0 一起做掉；采样率允许协商而声道数不允许，挑选偏好是「与源一致 > 源的整数倍 > 高于源的最小者 > 最大者」。重采样这类会静默出错的路径（比率算反、块边界丢样本、声道错位）必须能无头验证，`NullOutput::with_fixed_rate` 就是为此模拟「只支持单一采样率的设备」。

### 设计来源

UI 逐页复刻 Claude Design 项目「香农播放器设计简报」（10 页 + Token 文档，离线导出见 `docs/design/`），定稿方向「杏色·明快 2a」。设计稿页面已基本落地（曲库 / 专辑 / 歌手 / 歌曲 / 歌单 / 收藏 / 搜索 / 设置 / 歌词 / 首启引导，见 `src/components/` 同名目录），当前重心转向 Rust 后端接入（路线图见 README.md）；新页面应复用现有 Token、i18n 与 store 体系。

## 文档约定

- `README.md` / `README.en.md` 面向用户（产品介绍），开发内容一律放 `docs/DEVELOPMENT.md`；变更记录进 `CHANGELOG.md`（Keep a Changelog 格式）与 `docs/RELEASE_NOTES.md`。
- 所有文档不使用 emoji。
- 仓库根目录的 `AGENTS.md` 是本文件的镜像（仅首行称呼不同，供 Codex 读取）。**改 CLAUDE.md 就要同步改 AGENTS.md**，否则两个助手拿到的规则会分叉。
