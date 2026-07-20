# Windows Dolby Atmos/JOC 系统解码观测笔记

本文只关注带 Atmos/JOC 元数据的码流在 Windows 系统解码层暴露了什么。已经解码为固定
`7.1.4`、`9.1.6`、`22.2` 声道后如何进入系统空间音频渲染器，见
[Windows 固定多声道空间回放研究笔记](WINDOWS_SPATIAL_PLAYBACK_NOTES.md)。

## 目标

验证 Windows 在处理 Dolby Atmos 内容时，是否只是解码普通 5.1/6 声道声床，还是能在系统解码层得到超过 6 条的 spatial/object 数据。

本次实验不依赖听感，也不依赖功放、声吧或物理多声道设备显示。

## 样本特征与脱敏编号

真实文件名与路径不记录。当前只记录以下两个样本：

| 编号 | 普通声道布局 | 媒体信息报告的动态对象数量 | 采样率 | 备注 |
| --- | --- | ---: | ---: | --- |
| `sample-a` | 5.1 | 15 | 48 kHz | E-AC-3 JOC，M4A 封装 |
| `sample-e` | 5.1.2 | 15 | 48 kHz | E-AC-3 JOC，M4A 封装 |

两个样本的媒体信息均显示：

- 编码：E-AC-3 JOC
- 商业名称：Dolby Digital Plus with Dolby Atmos
- 采样率：48 kHz

这说明文件内包含 Atmos/JOC 对象信息，而不仅是普通声床。

## 普通 PCM 解码路径

使用 FFmpeg/ffprobe 或 Windows Media Foundation 请求普通 PCM/Float 输出时，结果为：

- 输出格式：float PCM
- `sample-a` 声道数：6
- `sample-e` 声道数：6

Media Foundation 普通解码观测：

```text
output subtype: Float PCM
decoded channels: 6
sample rate: 48000
```

结论：普通 PCM 路径只暴露 6 声道声床，不能证明对象元数据被展开。即使 `sample-e` 的媒体布局标记为 5.1.2，普通 PCM 请求下也仍然只返回 6 声道。

## Spatial Objects 解码路径

使用 Windows Media Foundation 请求以下输出 subtype：

```text
MFAudioFormat_Float_SpatialObjects
```

`sample-a` 与 `sample-e` 均成功返回 spatial-object 输出，而不是普通 PCM 输出。

观测结果：

```text
subtype: MFAudioFormat_Float_SpatialObjects
sample buffers per decoded sample: 17
non-empty float payload buffers: 16
each payload buffer: mono float, 1536 frames
```

其中一个 buffer 为零长度或非音频 payload，因此实际保存为 16 条 mono float buffer。

## 落盘分析

将 Media Foundation 返回的 spatial/object mono float buffers 交错保存为一个 16 声道 float WAV，仅用于分析，不代表传统扬声器布局。

以下“有明显音频能量”的数量来自完整分析区间的逐 buffer RMS，不是播放器可以实时展示的
“活跃声道数”，也不能据此推导节目在某一时刻的对象数量。

`sample-a` 保存后的分析结果：

```text
format: pcm_f32le
sample rate: 48000 Hz
channels: 16
duration: about 45.8 s
```

`sample-a` 每条 buffer 的能量统计显示：

- 暴露 16 条 buffer slot
- 其中 9 条有明显音频能量
- 第 7 条以后也有明显非静音信号
- 后若干 slot 为全零

典型能量结果：

```text
ch05 rms ~= -21 dBFS
ch07 rms ~= -20 dBFS
ch08 rms ~= -20 dBFS
ch09 rms ~= -33 dBFS
ch10 rms ~= -32 dBFS
```

`sample-e` 采用 stats-only 方式统计完整文件，不额外保存大体积 WAV。分析结果：

```text
spatial/object buffers: 16
duration: about 215.6 s
sample rate: 48000 Hz
```

`sample-e` 每条 buffer 的能量统计显示：

- 暴露 16 条 buffer slot
- 其中 15 条有明显音频能量
- 第 7 条以后也有明显非静音信号
- 第 1 条为全零

典型能量结果：

```text
ch03 rms ~= -28 dBFS
ch06 rms ~= -27 dBFS
ch10 rms ~= -22 dBFS
ch13 rms ~= -43 dBFS
ch15 rms ~= -40 dBFS
ch16 rms ~= -41 dBFS
```

## 结论

Windows 的普通 PCM 解码路径对 `sample-a` 与 `sample-e` 都只输出 6 声道，表现为普通声床。

但 Media Foundation 的 `MFAudioFormat_Float_SpatialObjects` 路径能从这两个 Atmos/JOC 样本中得到超过 6 条的 mono spatial/object float buffer。

因此，本次纯软件实验可以证明：

```text
系统解码层并非只能得到 6 声道声床；
在 spatial-object 输出路径下，Windows 可暴露超过 6 条的对象/空间音频 buffer。
```

需要注意：

- 这里的 16 声道 WAV 不是传统 16 声道扬声器布局。
- 它是为了分析方便，把多个 mono spatial/object buffer 交错保存为 WAV。
- 这不等同于最终播放设备输出了 12 声道或 16 声道。
- 它证明的是 renderer 前的系统解码结果包含超过 6 条可分析音频 buffer。
- 本实验没有取得对象坐标、尺寸、增益或随时间变化的轨迹元数据。

## 后续可做

1. 对更多 Atmos/JOC 样本重复同样测试。
2. 比较普通 6 声道 PCM 输出与 spatial-object buffer 混合后的差异。
3. 尝试识别各 buffer 与声床、对象、静音 slot 的对应关系。
4. 若要验证最终播放链路，需要另行观察 WASAPI endpoint 或 loopback 输出。
