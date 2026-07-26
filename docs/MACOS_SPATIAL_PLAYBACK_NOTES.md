# macOS 空间音频回放观测笔记

## 文档边界

本文记录 macOS 上「让系统按空间音频回放」的路径观测，与三份 Windows 笔记
（[Atmos/JOC 系统解码](ATMOS_DECODING_NOTES.md)、[AC-4 内部元数据](AC4_INTERNAL_METADATA_NOTES.md)、
[固定多声道空间回放](WINDOWS_SPATIAL_PLAYBACK_NOTES.md)）对称。

- 观测日期：2026-07-26。
- 环境：macOS 27.0（Darwin 27.0.0），Apple Silicon，输出端点为蓝牙耳机。
- 本文描述的是概念验证结果，**不代表 Shannon Player 已具备对应播放能力**。
- 验证工具是开发期诊断程序，不进入产品播放路径（符合架构约束的进程边界表）。
- **听感未验证**：本轮只确认了系统标识与链路可用性，没有做听感比对，
  也未排除 Apple 对原生 Atmos 流另有额外处理的可能。相关结论一律按「待验证」记录。

## 验证目标

1. macOS 的系统解码路径对 E-AC-3/JOC 暴露什么，对象信息是否保留。
2. 应用把多声道 PCM 交给系统时，系统是否按空间内容处理。
3. 系统把这条流**标识**成什么——「杜比全景声」还是「多声道」。第三点是产品问题：
   自己渲染成双耳立体声虽然听感相近，但系统空间音频开关会显示「不可用」，
   用户能直接看到降级。

## 解码路径的分野

同一个 E-AC-3/JOC 文件，两条系统 API 得到的结果不同：

| 解码路径 | 输出声道 | 对象信息 |
| --- | ---: | --- |
| `AudioConverter` / `afconvert` | 6 | 丢失，只有 core bed |
| `AVPlayer` + `MTAudioProcessingTap` | **12** | 已渲染进 7.1.4 |

判据沿用 Windows 笔记：拿到多于 bed 的路数，说明对象信息进入了解码结果。

**两条路径都返回成功，差异是静默的。** 走错 API 就得到 5.1 声床，没有任何错误提示——
这正是「处理不好会跌落到多声道」的具体机制。

各类输入的实测（容器声明 → tap 实际拿到）：

| 输入 | 容器声明 | tap 实际 |
| --- | --- | --- |
| E-AC-3/JOC，M4A 封装 | 6 ch `ec-3` | **12 ch** lpcm |
| APAC 12ch（7.1.4） | 12 ch `apac` | 12 ch lpcm |
| APAC 24ch（22.2） | 24 ch `apac` | 24 ch lpcm |
| FLAC 8ch（7.1） | 8 ch `flac` | 8 ch lpcm |

APAC 由系统解码，应用侧不需要实现 Apple 私有编码。

## 输出路径与系统标识

两条输出路径分别验证：

| 路径 | 构成 | 系统标识 |
| --- | --- | --- |
| A. AVPlayer 直通 | 系统解码 + 系统渲染 | 杜比全景声 |
| B. tap → ASBR | 系统解码 → 应用中转 12ch PCM → `AVSampleBufferAudioRenderer` | **杜比全景声** |

**B 的结果与预期相反。** 事前判断是：系统只看到一条多声道 PCM，应当标识为「多声道」。
实测两条都显示杜比全景声。

这意味着系统的标识依据不是「源文件是不是 Atmos」，而是当前渲染流的形态
（声道布局标签 + 允许空间化）。对实现的影响见下节。

作为对照，应用侧自行双耳化（`AUSpatialMixer` 渲染成 2ch 再输出）时，
系统空间音频显示「不可用」——系统拿不到可空间化的源，头部追踪也无从谈起。

## 系统标识由上游解码链路决定

对照实验（全部为方式 B，即 PCM 经应用中转后交给 ASBR）：

| 源 | 送进 ASBR 的形态 | 系统标识 |
| --- | --- | --- |
| E-AC-3/JOC | 12ch f32 + `Atmos_7_1_4` | **杜比全景声** |
| E-AC-3/JOC，中转时施加 −6 dB | 同上 | **杜比全景声**，空间定位正常，仅响度降低 |
| 12ch WAV（普通 PCM） | 12ch f32 + `Atmos_7_1_4` | 多声道 |
| 8ch FLAC（7.1） | 8ch f32 + `WAVE_7_1` | 多声道 |

第一行与第三行送给 ASBR 的形态**完全同构**，标识却不同。因此：

- 标识**不由** ASBR 侧收到的 PCM 或布局标签决定。
- 标识来自**上游 AVPlayer 解码链路**——系统知道该进程正在解码一个杜比全景声流，
  与应用是否把 PCM 取出中转无关。
- 系统标识是**诚实的**：它不会给确定无对象的内容标上全景声。
  最初担心的「给无对象内容显示全景声」不成立。

两条派生结论：

1. **应用可以在中间做处理**。施加 −6 dB 后标识不变、空间渲染正常、听感只是变轻，
   说明系统并未监视 PCM 的某种一致性。ReplayGain、音量斜坡、响度归一可以放在这一层。
2. **解码器因此被锁死**。macOS 上要保住全景声标识，解码必须走 AVPlayer；
   换成自建解码器或其他后端，标识随之消失。这不是质量取舍，是有无的问题，
   与「后端按能力探测选择」的一般原则冲突，需在实现时显式记录该约束。

即便标识可信，Shannon 自身仍**不应把系统 UI 的显示当作判据**：
是否为对象音频要由码流层面的 JOC 标记给出（见下节第 4 条），
系统显示只能用于交叉印证。理由是架构约束验收条件第 7 条——
未经证实的状态不得展示；判据在自己手里，结论才可复现。

## 触发系统空间化的条件

1. 输出走系统媒体播放栈（`AVSampleBufferAudioRenderer`），不是裸多声道硬件设备。
2. `CMSampleBuffer` 的 format description 带 `AudioChannelLayoutTag`。
   12 声道在 macOS 只有 `Atmos_7_1_4` 一个候选，不存在歧义。
3. `allowedAudioSpatializationFormats` 设为包含 multichannel。

`AVAudioSpatializationFormats` 只有 `None` / `MonoAndStereo` / `Multichannel` 三档，
**没有对象层的公开概念**。公开 API 的空间化模型是「多声道 → 空间化」。

## 静默失败模式

这条链路上的失败大多不报错，全部表现为「播放正常但没有声音」，
必须靠链路计数定位。已经踩到的三种：

1. **未注册 `requestMediaDataWhenReady`**：`isReadyForMoreMediaData` 不会转 true，
   直接 `enqueue` 等于一帧都没送进去。ASBR 不报错。
2. **planar 当交错处理**：tap 交出的是**非交错** f32（`mFormatFlags` 含
   `kAudioFormatFlagIsNonInterleaved`，实测 flags `0x29`），`AudioBufferList` 里是
   每声道一个 buffer。只取首个 buffer 就等于只送第 1 声道，而 format description
   仍声明 12 声道——数据量差 12 倍，ASBR 照收，播出静音。
3. **队列喂得太浅**（见 MacinRender 的既有实践）：ASBR 的欠载恢复会动态抬高它要求的
   最小 lead，喂得比 render deadline 浅会被静默丢弃，时钟继续跑而输出变哑。

因此诊断指标应覆盖：tap 回调次数、打包数、送出数、`isReadyForMoreMediaData`、
同步器 rate、源是否 planar 及实际交错声道数。

## 对 Shannon Player 的影响

「系统解码 + 应用管线 + 系统空间化」三者可以同时得到（中转施加增益的实验已确认
标识与空间渲染均不受影响；方式一与方式二之间的听感等价性仍待比对）：

```text
AVPlayer（系统解码，JOC → 7.1.4）
  → MTAudioProcessingTap 取 12ch f32（planar）
  → 应用管线（ReplayGain / 音量斜坡 / 进度）
  → AVSampleBufferAudioRenderer + Atmos_7_1_4 layout tag
  → 系统空间化与头部追踪
```

据此可以确定的实现边界：

1. macOS 的多声道与空间输出后端是 `AVSampleBufferAudioRenderer`，**不是 CPAL**。
   CPAL 的配置只有声道数，表达不了布局，而布局标签正是系统判断能否空间化的依据。
   对象音频路径上，解码端同时被锁定为 AVPlayer（见上节派生结论 2）。
2. ASBR 的缓冲参数与 CPAL 不是一个量级（ASBR 自带深队列，参考值约 1 秒，
   预填约 200 ms），不能沿用普通输出后端的参数。
3. 解码后端按能力探测选择：macOS 上 E-AC-3/JOC 与 APAC 交给系统，
   Symphonia 仍是跨平台基线（Linux 无等价系统解码器）。
4. 扫描期要判定「这是对象音频」不能依赖容器信息——`ec-3` + 6ch 看不出 JOC，
   需要解析 `dec3` box。运行时判据（tap 声道数 > 容器声明声道数）只在播放后可用。

## 当前未覆盖

- **A 与 B 两条路径之间的听感比对**：已确认的是「B 路径上施加 −6 dB 不劣化空间与标识」，
  尚未确认的是「A 与 B 本身是否等价」，即 Apple 对原生 Atmos 流是否另有处理。
  结论未定之前不应据此对外描述能力。
- 头部追踪在 B 路径下是否与 A 一致。
- 22.2、9.1.6 等其他布局经 ASBR 是否同样被认作空间内容。
- AC-4 输入（Windows 侧需要独立子进程与 OEM 解码器，macOS 侧未测）。
- 长时长播放的稳定性、seek 与设备切换行为；本轮 PoC 未实现完整的欠载滞后策略。
- 系统标识判定依据的直接反证（见「系统标识不能当作能力证据」一节）。
- 验证工具位于 `~/code/swift/SpatialCompare`（独立 git 仓库），
  界面文案刻意中性、技术差异不上主界面，用于分发做交叉验证。
