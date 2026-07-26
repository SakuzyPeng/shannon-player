# Windows AC-4 内部空间元数据兼容性笔记

## 文档边界

本文整理一个独立概念验证中与 Shannon Player 后续 Windows 音频后端有关的 AC-4 内部元数据
结论，重点是 AC-4 解码器与系统空间音频 metadata writer 之间的格式兼容问题。

本文已经做第二轮脱敏，不记录：

- 真实媒体文件名、节目名、文件路径和音频内容。
- 机器名、用户目录、输出端点名和端点标识。
- OEM 包身份、组件版本、二进制哈希、模块地址和精确系统构建号。
- 与故障无关的私有命令、对象数量、运行次数和性能数据。
- 概念验证工程的目录、构建产物和具体代码组织。

保留的私有格式标识、命令编号和字段宽度是复现问题所需的最小技术信息。文中的“已观测”只代表
匿名 Windows 11 环境中的一个 OEM AC-4 Media Foundation Transform（MFT）与受测 endpoint
组合，不能推导为所有 Windows 版本、驱动或设备的永久行为。

## 证据分级

- **运行时观测**：记录公开 COM 接口边界上的调用、参数宽度、HRESULT 和提交顺序，不保存音频。
- **静态分析**：用于确认 MFT 内部字段来源、命令含义和 endpoint 对命令长度的校验方式。
- **公开文档对照**：用于核对字段语义，不等同于在本机运行另一套解码器。
- **推断**：尚缺少接受完整 metadata 的独立渲染路径，必须明确标注，不能写成平台契约。

## 问题摘要

受测 MFT 能把 AC-4 解码为 `MFAudioFormat_Float_SpatialObjects`，并为对象缓冲创建逐帧
metadata item。问题不在“是否解出了对象 PCM”，而在写入第一个节目级响度命令时：

```text
AC-4 MFT 提交 command 23，长度 8 bytes
              ↓
endpoint metadata writer 只接受长度 1 byte
              ↓
WriteNextItemCommand 返回 E_INVALIDARG
              ↓
MFT 放弃当前 item，未执行 Close
              ↓
后续 command 9 坐标没有机会提交
```

因此可以同时看到对象音频缓冲已经产生、坐标却始终为零或不可读。它不是“文件没有对象元数据”的
充分证据，也不能只用普通 PCM 声道数判断。不同电脑使用相同程序和文件仍可能表现不同，因为实际
命令字典与所选 endpoint 的系统空间音频属性有关。

## Metadata 格式边界

受测 MFT 在输出 media type 中声明：

```text
名称：DOLBY_ATMOS_AC4_METADATA_V1_0
GUID：{75041BC7-2D36-4BDE-ADC3-D0A323E0376D}
```

与问题有关的公开接口调用顺序可缩写为：

```text
MFT ProcessOutput
  -> IMFSpatialAudioSample
  -> IMFSpatialAudioObjectBuffer::GetMetadataItems
  -> ISpatialAudioMetadataWriter::Open
  -> ISpatialAudioMetadataWriter::WriteNextItem
  -> ISpatialAudioMetadataWriter::WriteNextItemCommand
  -> ISpatialAudioMetadataWriter::Close
  -> Windows Spatial Audio Renderer
```

metadata command 的编号和 value 长度由 metadata format 定义，不能把另一种 Atmos 格式的
命令编号或结构直接套到 AC-4。早期探针只寻找一个携带三个 `float` 的假定坐标命令，因此即使
AC-4 坐标存在也会报告零；这是探针解析错误，不是码流结论。

## Command 9：对象位置

在上述 AC-4 metadata format 中，command `9` 的 value 固定为 6 bytes：

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 2 | X，little-endian `uint16` |
| 2 | 2 | Y，little-endian `uint16` |
| 4 | 2 | Z，little-endian `uint16` |

每个分量使用无符号 Q15 风格的归一化范围：

```text
normalized = raw / 32767.0
```

静态分析结合固定位置覆盖确认，X 表示左到右，Y 表示前到后，Z 表示下到上。command `9`
既可携带动态位置，也可为借用对象槽位的固定扬声器位置赋值，因此“出现 command 9”不等于
“该时刻存在同等数量的动态源对象”。

## Command 23：节目响度描述

受测 MFT 在第一对象的首个 metadata item 中先提交 command `23`。其 decoder-side value 为
8 bytes、little-endian：

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | `dialnorm_bits`，`uint32` |
| 4 | 4 | `loud_prac_type`，`uint32`，由较小字段零扩展 |

`dialnorm_bits` 来自 AC-4 `basic_metadata`，换算关系为：

```text
dialnorm dBFS = -0.25 * dialnorm_bits
```

`loud_prac_type` 来自进一步响度信息，描述响度测量或监管实践。两者都是节目级响度信息，不是
对象坐标、对象数量、声道标签或时间偏移。一个与故障直接相关的有效初始值为 `{124, 0}`，表示
`-31 dBFS` dialnorm 且未指明响度实践。

## 长度不匹配的证据

隔离的 writer 探针为每次试验创建新的 metadata items 和 writer，只测试命令 value 长度，不
打开媒体文件或启动播放。结果为：

| Command | MFT 提交长度 | Endpoint 接受长度 | 结果 |
| --- | ---: | ---: | --- |
| `23` | 8 bytes | 1 byte | 8 bytes 返回 `E_INVALIDARG` |
| `9` | 6 bytes | 6 bytes | 长度匹配并可接受 |

一字节 command `23` value 可以通过 writer 的边界校验，但这只证明 schema 长度，不证明该
byte 的私有语义。endpoint 可能采用只携带原始七位 `dialnorm_bits` 的旧 schema，这是目前最
具体的兼容性假说，仍不是已证实映射。

系统 writer 按所选 metadata format 的命令字典严格比较 value 长度。该字典来自 endpoint 的
空间音频属性，而不是由媒体文件或播放器固定提供。因此，不同系统组件或 endpoint 暴露不同字典，
可以解释“同一可执行文件与同一媒体在一台电脑成功、另一台失败”。预检应把这种情况报告为
endpoint metadata schema 不兼容，而不是笼统地归因于文件。

## 失败链

一次受影响的 metadata item 按以下顺序失败：

1. `Open` 与首个 `WriteNextItem` 成功。
2. command `23` 的 8-byte value 被 writer 以 `E_INVALIDARG` 拒绝。
3. MFT 中止该 item，没有调用 `Close`。
4. 后续对象再打开 metadata 时得到 `SPTLAUD_MD_CLNT_E_ITEMS_ALREADY_OPEN`。
5. reader 侧读取 metadata 时得到 `SPTLAUD_MD_CLNT_E_ITEMS_LOCKED_FOR_WRITING`。
6. 排在后面的 command `9` 坐标没有被提交。

这条链解释了“对象 PCM 存在，但空间定位集中或 metadata 预检失败”的表面矛盾。听感上的中央
聚集只能作为症状，不能替代命令级观测。

## 受限兼容实验

概念验证曾在隔离进程中使用一个严格限定的兼容分支：先调用原 writer，仅当 command ID 为
`23`、value 长度为 8 bytes 且原调用返回 `E_INVALIDARG` 时，才向 MFT 返回成功。其他命令、
长度和 HRESULT 均原样传递；command `9` 不被合成、转换或改写。

启用该分支后，后续 command `9` 与 metadata `Close` 可以到达 writer，Media Session 时钟也
能持续前进。这证明 command `23` 的长度冲突阻塞了后续坐标提交，但该实验有明确损失：writer
没有存储被拒绝的节目响度描述。因此它是“恢复空间路径”的兼容验证，不是完整 metadata 透传，
也不是 Shannon Player 可以无条件采用的生产方案。

由于受测 writer 实现的拦截范围不是单一实例，概念验证把整个 AC-4 路径限制在可丢弃的独立进程
中。这个实现细节只用于控制实验风险，不改变 metadata schema 不匹配这一根因。

## 响度概念不能合并

解码器目标参考电平、码流 `dialnorm`、`loud_prac_type` 和 DRC 是不同概念。已有测量表明，受测
MFT 会在提交空间 metadata 之前把所选目标参考策略应用到对象 PCM。command `23` 随后仍会
转发节目级响度描述。

Fluendo 对其 PADS-based `fluac4dec` 的公开文档也分别暴露 `dialnorm`、
`loudness-practice-type`、`output-ref-level` 和 `drc-mode`，为字段分离提供了外部旁证。不过本次
没有取得该商业插件进行运行时测试，它不能证明 Microsoft 私有 command `23` 的一字节 schema。

因此，丢弃 command `23` 后不能再根据 `dialnorm_bits` 在播放器中施加固定增益；这可能与解码器
已经完成的参考电平处理叠加，造成二次响度补偿。AC-4 的目标参考电平也不应直接命名为 E-AC-3
意义上的 LINE/RF DRC 模式。

## 对 Shannon Player 的约束

1. AC-4 能力探测应分别记录解码器、metadata format、endpoint writer schema 和 renderer，不能
   用一个“支持 Atmos”布尔值覆盖整条链路。
2. 文件解析或 MFT 输出成功不代表 metadata renderer 可用；预检错误必须指出失败阶段、命令、
   value 长度和原始 HRESULT。
3. 只有观察到 position command 成功、metadata item 成功 `Close` 且渲染时钟持续前进，才可把
   原生空间 metadata 路径标记为已激活。
4. UI 必须区分对象缓冲、metadata item、坐标命令和源对象。槽位数或累计提交次数不能显示为
   “当前动态对象数量”。
5. 普通 PCM 回退若存在，必须明确显示为另一条输出路径，不能继续宣称 AC-4 对象 metadata 已被
   renderer 使用。
6. 兼容策略必须按 metadata format、command ID、value 长度和原 HRESULT 精确限定，并在接受
   完整命令的平台上自动成为 no-op。
7. 诊断日志只记录结构、计数、阶段和 HRESULT。默认不记录原始音频、完整坐标轨迹、设备标识或
   媒体身份。

## 尚未解决

- endpoint 接受的一字节 command `23` 是否就是原始 `dialnorm_bits`。
- 接受完整 command `23` 的 renderer 会如何使用响度实践字段。
- 其他私有命令的含义。它们与当前阻塞问题无关，因此本文有意不展开。
- Windows、驱动和 endpoint 组合之间的 schema 版本矩阵。
- 可独立执行的 AC-4 参考解码器与 renderer A/B；目前只有公开接口文档对照。

## 参考资料

- [IMFSpatialAudioObjectBuffer](https://learn.microsoft.com/windows/win32/api/mfspatialaudio/nn-mfspatialaudio-imfspatialaudioobjectbuffer)
- [ISpatialAudioMetadataWriter](https://learn.microsoft.com/windows/win32/api/spatialaudiometadata/nn-spatialaudiometadata-ispatialaudiometadatawriter)
- [ISpatialAudioMetadataWriter::WriteNextItemCommand](https://learn.microsoft.com/windows/win32/api/spatialaudiometadata/nf-spatialaudiometadata-ispatialaudiometadatawriter-writenextitemcommand)
- [ISpatialAudioMetadataWriter::Close](https://learn.microsoft.com/windows/win32/api/spatialaudiometadata/nf-spatialaudiometadata-ispatialaudiometadatawriter-close)
- [ISpatialAudioMetadataClient](https://learn.microsoft.com/windows/win32/api/spatialaudiometadata/nn-spatialaudiometadata-ispatialaudiometadataclient)
- [ETSI TS 103 190-1 V1.4.1](https://www.etsi.org/deliver/etsi_ts/103100_103199/10319001/01.04.01_60/ts_10319001v010401p.pdf)
- [Fluendo Dolby AC-4 Decoder](https://fluendo.com/products/ac4-decoder/)

