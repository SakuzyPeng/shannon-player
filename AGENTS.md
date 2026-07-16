# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

香农播放器（Shannon Player）：Tauri 2 + React 19 桌面本地音乐播放器，AGPL-3.0-only。文档与代码注释以简体中文为主。

## 常用命令

```bash
pnpm install          # 安装前端依赖（包管理器固定用 pnpm）
pnpm tauri dev        # 启动桌面应用（自动拉起 Vite dev server 于 :1420）
pnpm dev              # 仅前端，浏览器访问 http://localhost:1420
pnpm build            # 类型检查（tsc --noEmit）+ 前端产物构建（vite）
pnpm tauri build      # 打包桌面应用
```

目前没有测试套件；`pnpm build` 是主要的正确性校验（i18n 漏键、类型错误都会在这里暴露）。

验证 UI 效果用 Playwright MCP 指向 `http://localhost:1420` 截图——Tauri 原生窗口截图受 macOS 屏幕录制权限限制，而 Vite dev server 渲染的是同一份前端。注意 Radix 菜单需要真实 pointer 事件，合成 `.click()` 不会触发，需用 browser_click。

## 架构

双层结构，通过 pnpm scripts 串联：

- **`src/`** —— React 19 + TypeScript + Vite 前端，承载全部 UI 与交互逻辑。
- **`src-tauri/`** —— Rust 外壳（cargo workspace 成员）。当前只负责窗口：无边框（`decorations: false`）、自绘 macOS 交通灯（`src/components/window/TrafficLights.tsx` 经 `@tauri-apps/api/window` 调原生窗口控制，权限声明在 `src-tauri/capabilities/default.json`）。后期计划承接音频播放与本地曲库扫描。

### 关键机制（跨文件才能看清的部分）

**设计 Token 与主题切换**：`src/index.css` 中 `:root` 定义浅色 CSS 变量（`--bg`/`--tx`/`--ac` 等），`[data-theme="dark"]` 整套覆盖；Tailwind v4 的 `@theme inline` 把它们映射为 `--color-*`，因此 `bg-bg`、`text-tx` 等工具类会随 `<html data-theme>` 实时换肤。`src/hooks/useApplyTheme.ts` 负责写 `data-theme` 并监听系统偏好（"system" 模式）。**组件里禁止写字面色值**，一律走语义变量。

**状态**：Zustand 两个 store——`src/store/player.ts`（播放器领域：队列/进度/音频设备/循环，类型全部来自 `src/types/player.ts` 的强类型领域模型，不用散装字段）、`src/store/ui.ts`（主题/视图/导航/语言）。

**国际化（第一天就做，强类型）**：`src/i18n/messages.ts` 的 `Messages` 接口是全部文案键的单一来源，任一语言字典漏键会编译报错。组件内经 `useT()` 取翻译，支持 `{var}` 插值。新增任何界面文案必须加 key，禁止硬编码；专辑名、歌手名等**内容不翻译**。

**语言范围承诺（重要约束）**：对外只承诺简体中文 + English。`src/data/library.ts` 的 `LANGUAGES` 与 `src/i18n/index.ts` 的 `detectSystemLocale()` 只暴露这两者；zh-Hant / ja 的词条在 `messages.ts` 中备好但**不得**加入 UI 菜单或系统语言解析，文档中也不得宣传，除非用户明确解除该限制。

**数据**：`src/data/library.ts` 是曲库种子数据（来自设计稿），后期由 Rust 后端扫描本地曲库替换。

**滚动**：滚动「手感」交还各平台原生（macOS 触控板橡皮筋、Windows/Linux 滚轮惯性各自沿用系统实现），只统一「视觉」。`src/hooks/useElasticScroll.ts`（名称沿用，实为「原生滚动 + 自绘滚动条」）不再拦截 wheel、不再自积分物理，仅：容器用 `.no-scrollbar` 隐藏系统滚动条，并按原生 `scroll` 事件的 scrollTop/scrollHeight 直接映射绘制一份跨平台一致的 6px thumb（静止 0.9s 后淡出，内容未溢出不显示）。返回签名 `{ scrollerRef, innerRef, thumbRef, onScroll }` 不变，`innerRef` 现仅作内容容器，但保留 `will-change:transform`——它把内容提升为一张缓存的合成层，令 superellipse 圆角 + 多重内阴影的封面卡只光栅化一次、滚动时纯合成（去掉会导致专辑网格滚动掉帧）。曾有一版自定义速度积分 + 橡皮筋引擎，因难以在各平台/输入设备上都贴合原生肌肉记忆，权衡后回退为原生手感。

**窗口自适应（布局戒律）**：窗口可任意拉伸，需保证的区间是 [980×640, ∞)（下限由 `src-tauri/tauri.conf.json` 的 minWidth/minHeight 兜底，设计稿画板 1180×760 只是默认尺寸）。规则：卡片网格一律 `repeat(auto-fill, minmax(…, 1fr))`，禁止固定列数（`grid-cols-4` 这类会在超宽下把封面撑到失衡）；文本容器一律 `truncate` + `min-w-0`；滚动区一律 `absolute inset-0 overflow-auto` 套 `min-h-0 flex-1`（高度变矮只增加滚动，不裁内容）；长文页面用 `max-w` 居中；对高度敏感的固定尺寸元素（如歌词页封面）用 `min(设计值, Nvh)` 收敛。改动布局后用 Playwright 在 980×640 与 1920×1080 各截一轮，并断言 `document.documentElement.scrollWidth <= clientWidth`（零横向溢出）。

### 设计来源

UI 逐页复刻 Claude Design 项目「香农播放器设计简报」（10 页 + Token 文档，离线导出见 `docs/design/`），定稿方向「杏色·明快 2a」。当前已实现曲库主界面与专辑详情页，其余页面按路线图（见 README.md）逐页迭代；新页面应复用现有 Token、i18n 与 store 体系。

## 文档约定

- `README.md` / `README.en.md` 面向用户（产品介绍），开发内容一律放 `docs/DEVELOPMENT.md`；变更记录进 `CHANGELOG.md`（Keep a Changelog 格式）与 `docs/RELEASE_NOTES.md`。
- 所有文档不使用 emoji。
