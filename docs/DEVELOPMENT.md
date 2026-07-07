# 开发文档

面向贡献者的技术说明。用户向的介绍见根目录 [README](../README.md)。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 桌面壳 | Tauri 2（无边框窗口 + 自绘 macOS 交通灯，接入原生窗口控制） |
| 前端 | React 19 + TypeScript + Vite |
| 样式 | Tailwind v4 + 设计 Token（CSS 变量，浅/深/系统三态） |
| 状态 | Zustand（强类型播放器领域模型：队列 / 进度 / 音频设备 / 循环模式） |
| 动画 | Framer Motion |
| 菜单 | Radix UI（右键菜单 / 下拉菜单，键盘导航与无障碍开箱即用） |
| 国际化 | 自建类型安全 i18n（当前简体中文与 English，架构可扩展更多语言） |
| 音频 | 前期 HTMLAudioElement / Web Audio，后期由 Rust 后端增强 |
| 歌词 | 规划接入 AMLL（Apple Music-like Lyrics） |

## 环境要求

- Node ≥ 20、pnpm ≥ 10
- Rust stable ≥ 1.77、`cargo tauri`（`cargo install tauri-cli` 或使用 `pnpm tauri`）
- macOS：Xcode Command Line Tools

## 常用命令

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 启动 Tauri（自动拉起 Vite dev server 于 :1420 并打开窗口）
pnpm dev              # 仅前端，浏览器预览 http://localhost:1420
pnpm build            # 前端类型检查（tsc）+ 产物构建（vite）
pnpm tauri build      # 打包桌面应用
```

## 目录结构

```
src/                    前端 React 应用
  components/            组件（layout / library / player / window / common）
  store/                 Zustand（player.ts 播放器领域状态、ui.ts 界面状态）
  types/                 播放器领域类型（Track / QueueItem / Lyrics / AudioDevice …）
  i18n/                  国际化（messages.ts 消息目录、index.ts 运行时）
  data/                  曲库种子数据（后期由 Rust 后端扫描替换）
  hooks/                 useApplyTheme（主题）、useElasticScroll（弹性滚动物理）
  index.css              Tailwind 入口 + 设计 Token
src-tauri/               Tauri Rust 外壳（窗口、权限、后续音频后端）
docs/                    开发文档
```

## 约定

- **设计 Token**：颜色一律经语义化 CSS 变量引用（`--bg`/`--tx`/`--ac` …），禁止在组件里写字面色值；深色整套替换，见 `src/index.css`。
- **国际化**：所有界面文案必须加入 `src/i18n/messages.ts` 的 `Messages` 接口并经 `useT()` 渲染，禁止硬编码；专辑、歌手等**内容**不进 i18n。漏键会在编译期报错。
- **状态**：播放器领域状态（队列、进度、歌词时间轴、音频设备）走 `src/types/player.ts` 的强类型模型，不用散装字段。

## 设计来源

Claude Design 项目「香农播放器设计简报」，共 10 个页面 + 设计 Token 文档，定稿方向
「杏色 · 明快 2a」。离线参考包已整理在 [docs/design](design/README.md)；当前已实现曲库主界面
2a，其余页面逐页迭代。
