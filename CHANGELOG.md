# 更新日志

本项目的所有重要变更都会记录在此文件。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增
- Tauri 2 + React 19 + TypeScript + Vite 工程骨架。
- 设计 Token 层：Tailwind v4 + CSS 变量，浅色 / 深色两套语义配色。
- 曲库主界面（「杏色 · 明快 2a」设计）：
  - 图标栏：导航、语言菜单、浅色 / 深色 / 跟随系统三态外观切换。
  - 专辑网格视图（封面 hover 浮现播放键）与详情列表视图，可切换。
  - 悬浮播放条：播放 / 暂停、上一首 / 下一首、随机、循环、进度、音量、收藏。
  - 自绘 macOS 交通灯，接入真实窗口控制（关闭 / 最小化 / 最大化）。
  - 弹性滚动物理与自绘滚动条。
  - Radix UI 右键菜单（键盘导航）与语言下拉菜单。
- 强类型播放器领域模型：播放队列、播放进度、音频设备、循环模式（`src/types/player.ts`）。
- 类型安全的国际化：当前提供简体中文与 English，界面文案实时切换（i18n 架构可扩展更多语言）。

### 说明
- 尚未接入真实音频播放与本地曲库扫描；曲库为演示种子数据。

[未发布]: https://example.com/shannon-player/compare/HEAD
