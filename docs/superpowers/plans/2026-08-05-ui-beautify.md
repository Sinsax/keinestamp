# UI 美化（慧音双形态主题）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Keinestamp 前端界面从浅蓝灰金色主题整体换成「上白泽慧音双形态」浅色简约风（白底 + 正红主色 + 深蓝/冰蓝人形色 + 墨绿白泽色），并加入侧栏折叠、时间线拖拽调宽两个布局微调与适度动画。

**Architecture:** 纯前端改动（无 Rust 侧变更）。核心是重建 `app.scss` 设计令牌（颜色/圆角/阴影/动画），各 `.svelte` 组件通过 scoped style 覆盖局部细节；`App.svelte` 新增侧栏折叠与时间线宽度状态（localStorage 持久化）。Logo（TitleBar 内联 SVG + `public/keinestamp.svg`）换红白配色。

**Tech Stack:** Svelte 5（`$state`/`$props`/`$effect`）、SCSS 变量、原生 Pointer Events 拖拽、localStorage。无新依赖。

## Global Constraints

- 不引入任何 UI 框架/动画库，纯 CSS 动画
- 不修改 Rust 后端、Tauri 命令、`api.ts` 接口签名
- 验证命令固定为：`cd frontend; npm run check`（svelte-check + tsc）
- 设计令牌值以本计划 Task 1 为准，组件内不得再出现硬编码颜色（`#cba6f7`、`#1e1e2e`、`#4caf88`、`#e06c75`、`#e5c07b`、`#648cff`、`#4ade80`、`#f87171`、`#7c3aed`、`#cba6f7` 等旧色值全部替换为令牌）
- 每个任务以 `npm run check` 通过 + 视觉目检为完成标准；视觉目检项见各任务 Checklist
- 每个任务结束后 git commit（消息格式：`feat(ui): ...`）
- 根目录 `package-lock.json`（SakiStep 残留）永不提交
- Svelte scoped style 优先级高于全局 `app.scss`，组件内样式可覆盖全局
- 全局 keyframes（`fade-in`/`dialog-in`/`toast-in`）定义在 app.scss；组件 scoped style 中直接引用这些名称（Svelte 不会改写全局 keyframe 名引用）

---

### Task 1: 设计令牌与全局样式（app.scss 整体重写）

**Files:**
- Rewrite: `frontend/src/app.scss`（全部 261 行替换）

**Interfaces:**
- Produces: 全局 CSS 变量 `--accent/--accent-hover/--accent-soft/--blue/--blue-hover/--blue-soft/--ze/--ze-hover/--ze-soft/--ice/--ice-soft/--bg/--surface/--surface2/--surface3/--border/--text/--text-dim/--success/--warning/--error/--accent2/--radius/--radius-lg/--shadow-sm/--shadow-md/--shadow-lg/--mono/--sidebar-w/--sidebar-w-collapsed/--action-h/--status-h/--ease`；全局 keyframes `fade-in`、`dialog-in`、`toast-in`；全局类 `.sidebar.collapsed`、`.dialog-overlay/.dialog`、`.detail-overlay/.detail-panel`、`.toast-container/.toast(.success/.error/.info)`、`.empty-state/.big`、`.entry-card/.tagged/.active`、`.toolbar`、`.status-bar`、`.action-bar`

- [ ] **Step 1: 用以下完整内容替换 `frontend/src/app.scss`**

```scss
:root {
  /* 上白泽慧音 双形态 */
  --accent: #d94747;
  --accent-hover: #c63b3b;
  --accent-soft: #fbeded;
  --blue: #2e4a6b;
  --blue-hover: #243b57;
  --blue-soft: #ebf1f7;
  --ze: #3e6b52;
  --ze-hover: #335a44;
  --ze-soft: #eaf2ec;
  --ice: #7fa8ce;
  --ice-soft: #eff5fb;
  /* 白 */
  --bg: #f4f6f8;
  --surface: #ffffff;
  --surface2: #edf0f4;
  --surface3: #dce1e8;
  --border: #e3e7ed;
  --text: #1e2833;
  --text-dim: #6b7684;
  --success: #3e6b52;
  --warning: #b0803a;
  --error: #c94343;
  --accent2: var(--accent);
  /* 视觉语言 */
  --radius: 10px;
  --radius-lg: 14px;
  --shadow-sm: 0 1px 2px rgba(30, 40, 51, 0.06);
  --shadow-md: 0 4px 12px rgba(30, 40, 51, 0.10);
  --shadow-lg: 0 16px 48px rgba(30, 40, 51, 0.18);
  --mono: ui-monospace, 'Cascadia Mono', 'Segoe UI Mono', Consolas, monospace;
  /* 布局 */
  --sidebar-w: 240px;
  --sidebar-w-collapsed: 44px;
  --action-h: 44px;
  --status-h: 28px;
  --ease: cubic-bezier(0.22, 0.61, 0.36, 1);
}

* { margin: 0; padding: 0; box-sizing: border-box; }

html, body, #app {
  height: 100%; width: 100%;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'PingFang SC', 'Microsoft YaHei', sans-serif;
  background: var(--bg);
  color: var(--text);
  overflow: hidden;
  user-select: none;
  -webkit-user-select: none;
}

button {
  font-family: inherit;
  cursor: pointer;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text);
  padding: 6px 14px;
  border-radius: 8px;
  font-size: 12px;
  transition: background 0.15s var(--ease), border-color 0.15s var(--ease),
    box-shadow 0.15s var(--ease), transform 0.1s var(--ease);
}
button:hover:not(:disabled) { background: var(--surface2); border-color: var(--surface3); }
button:active:not(:disabled) { transform: scale(0.97); }
button:disabled { opacity: 0.35; cursor: default; pointer-events: none; }
button.accent {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
  font-weight: 600;
}
button.accent:hover:not(:disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
  box-shadow: 0 2px 8px rgba(217, 71, 71, 0.35);
}

input, textarea, select {
  font-family: inherit;
  background: var(--surface);
  border: 1px solid var(--border);
  color: var(--text);
  padding: 7px 11px;
  border-radius: 8px;
  font-size: 13px;
  outline: none;
  width: 100%;
  transition: border-color 0.15s var(--ease), box-shadow 0.15s var(--ease);
}
input:focus, textarea:focus, select:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px rgba(217, 71, 71, 0.12);
}

::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--surface3); border-radius: 4px;
  border: 2px solid transparent; background-clip: content-box;
}
::-webkit-scrollbar-thumb:hover { background: var(--text-dim); background-clip: content-box; }

.app-layout { display: flex; flex-direction: column; height: 100%; }
.body { display: flex; flex: 1; overflow: hidden; }

/* ---- 侧栏 ---- */
.sidebar {
  width: var(--sidebar-w);
  background: var(--surface);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition: width 0.2s var(--ease);
}
.sidebar.collapsed { width: var(--sidebar-w-collapsed); }
.sidebar-header {
  padding: 10px 14px;
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 1.2px;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}
.sidebar-list { flex: 1; overflow-y: auto; }
.sidebar-item {
  padding: 7px 12px;
  font-size: 12px;
  cursor: pointer;
  border-bottom: 1px solid var(--border);
  transition: background 0.12s;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sidebar-item:hover { background: var(--surface2); }
.sidebar-item.active { background: var(--accent-soft); border-left: 3px solid var(--accent); }

/* ---- 主区 ---- */
.main-area { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  min-height: 38px;
}
.entries { flex: 1; overflow-y: auto; padding: 10px; }

.entry-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px;
  margin-bottom: 8px;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  transition: box-shadow 0.15s var(--ease), border-color 0.15s var(--ease),
    transform 0.15s var(--ease);
}
.entry-card:hover { border-color: var(--accent); box-shadow: var(--shadow-md); transform: translateY(-1px); }
.entry-card.active { border-color: var(--accent); background: #fffdfd; }
.entry-card.tagged { border-left: 3px solid var(--tag-color, var(--border)); }
.entry-card .top {
  display: flex; justify-content: space-between; align-items: baseline;
  margin-bottom: 3px;
}
.entry-card .hash { font-size: 11px; color: var(--text-dim); font-family: var(--mono); }
.entry-card .date { font-size: 11px; color: var(--text-dim); }
.entry-card .msg { font-size: 13px; font-weight: 500; margin-bottom: 3px; }
.entry-card .files {
  font-size: 11px; color: var(--text-dim);
  display: flex; gap: 4px; flex-wrap: wrap;
}
.entry-card .files span {
  background: var(--surface2);
  padding: 1px 6px;
  border-radius: 4px;
}

/* ---- 弹窗 ---- */
.dialog-overlay, .detail-overlay {
  position: fixed;
  inset: 0;
  background: rgba(30, 40, 51, 0.35);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
  animation: fade-in 0.15s var(--ease);
}
.dialog, .detail-panel {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 24px;
  box-shadow: var(--shadow-lg);
  animation: dialog-in 0.2s var(--ease);
}
.dialog h3 { margin-bottom: 12px; font-size: 16px; }
.dialog .field { margin-bottom: 10px; }
.dialog .field label { display: block; font-size: 11px; color: var(--text-dim); margin-bottom: 3px; }
.dialog .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.detail-panel h2 { font-size: 17px; margin-bottom: 4px; }
.detail-panel .meta { font-size: 12px; color: var(--text-dim); margin-bottom: 12px; }
.detail-panel .field { margin-bottom: 10px; }
.detail-panel .field label { display: block; font-size: 10px; color: var(--text-dim); margin-bottom: 2px; text-transform: uppercase; letter-spacing: 0.8px; }
.detail-panel .field .value { font-size: 13px; word-break: break-all; }
.detail-panel .field .value.files-list { display: flex; gap: 4px; flex-wrap: wrap; }
.detail-panel .field .value.files-list span { background: var(--surface2); padding: 2px 8px; border-radius: 4px; font-size: 12px; }

/* ---- 底部操作栏 / 状态栏 ---- */
.action-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  height: var(--action-h);
  padding: 0 8px;
  background: var(--surface);
  border-top: 1px solid var(--border);
}
.action-bar .spacer { flex: 1; }
.status-bar {
  height: var(--status-h);
  display: flex;
  align-items: center;
  padding: 0 12px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 11px;
  color: var(--text-dim);
}

/* ---- Toast ---- */
.toast-container {
  position: fixed;
  bottom: calc(var(--action-h) + var(--status-h) + 12px);
  right: 16px;
  z-index: 300;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.toast {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-left: 3px solid var(--ice);
  border-radius: var(--radius);
  padding: 10px 14px;
  font-size: 12px;
  box-shadow: var(--shadow-md);
  animation: toast-in 0.2s var(--ease);
  max-width: 340px;
}
.toast.success { border-left-color: var(--success); }
.toast.error { border-left-color: var(--error); }
.toast.info { border-left-color: var(--ice); }

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}
@keyframes dialog-in {
  from { opacity: 0; transform: scale(0.96) translateY(8px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}
@keyframes toast-in {
  from { opacity: 0; transform: translateX(24px); }
  to { opacity: 1; transform: translateX(0); }
}

/* ---- 空状态 ---- */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-dim);
  font-size: 14px;
  gap: 10px;
}
.empty-state .big {
  width: 72px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--ice-soft);
  border-radius: 50%;
}
```

- [ ] **Step 2: 运行检查**

Run: `cd frontend; npm run check`
Expected: 无错误（app.scss 语法由 sass 插件编译，svelte-check 校验 ts）。

- [ ] **Step 3: 目检基线**

Run: `cd frontend; npm run dev`（或 `cargo tauri dev`）后确认应用仍可启动、布局不破、按钮/输入框/滚动条已变为白底红主色。由于后续任务逐步覆盖组件细节，此处只需确认无运行时错误。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/app.scss
git commit -m "feat(ui): 重建设计令牌与全局样式（慧音双形态配色）"
```

---

### Task 2: TitleBar 样式与 Logo 换色（含 favicon）

**Files:**
- Modify: `frontend/src/lib/TitleBar.svelte`（Logo SVG 换色 + `<style>` 整块替换）
- Rewrite: `frontend/public/keinestamp.svg`（15 行全替换）

**Interfaces:**
- Consumes: Task 1 令牌（`--accent`、`--accent-soft`、`--error`、`--surface2`、`--border`、`--text`、`--text-dim`）
- Produces: 红白 Logo（TitleBar 内联 SVG 与 `public/keinestamp.svg` 内容一致）

- [ ] **Step 1: 替换 TitleBar.svelte 的 Logo SVG（第 35-49 行）**

```svelte
      <path d="M28 22 C44 14, 56 14, 72 22 C84 34, 84 66, 72 78 C56 86, 44 86, 28 78 C16 66, 16 34, 28 22Z" fill="#D94747"/>
      <path d="M33 28 C22 40, 22 60, 33 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M44 24 C32 40, 32 60, 44 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M56 24 C44 40, 44 60, 56 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M67 28 C56 40, 56 60, 67 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <circle cx="40" cy="40" r="3" fill="#FFE3E3"/>
      <circle cx="44" cy="55" r="3.5" fill="#9FC3E3"/>
      <circle cx="50" cy="70" r="3" fill="#FFE3E3"/>
      <circle cx="52" cy="38" r="3.5" fill="#9FC3E3"/>
      <circle cx="58" cy="53" r="3" fill="#FFE3E3"/>
      <circle cx="62" cy="68" r="3.5" fill="#9FC3E3"/>
      <path d="M46 16 C48 10, 52 6, 58 8" stroke="#E8B4B4" stroke-width="3" fill="none" stroke-linecap="round"/>
      <path d="M58 8 C62 6, 64 8, 66 7" stroke="#E8B4B4" stroke-width="2.5" fill="none" stroke-linecap="round"/>
```

- [ ] **Step 2: 替换 TitleBar.svelte 的 `<style>` 整块（第 67-136 行）**

```css
  .titlebar {
    display: flex;
    align-items: center;
    height: 34px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    user-select: none;
  }
  .tb-brand {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 100%;
  }
  .tb-icon {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }
  .tb-title {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.3px;
    white-space: nowrap;
  }
  .tb-subtitle {
    font-size: 10px;
    color: var(--text-dim);
    margin-left: 10px;
    white-space: nowrap;
  }
  .tb-spacer {
    flex: 1;
    height: 100%;
  }
  .tb-controls {
    display: flex;
    gap: 2px;
    margin-right: -4px;
    height: 100%;
    align-items: center;
    flex-shrink: 0;
  }
  .tb-btn {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    -webkit-app-region: no-drag;
    transition: background 0.1s, color 0.1s;
  }
  .tb-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tb-close:hover {
    background: var(--error);
    color: #fff;
  }
```

- [ ] **Step 3: 用同色替换 `frontend/public/keinestamp.svg`（favicon，第 2-14 行同理）**

```svg
<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
  <path d="M28 22 C44 14, 56 14, 72 22 C84 34, 84 66, 72 78 C56 86, 44 86, 28 78 C16 66, 16 34, 28 22Z" fill="#D94747"/>
  <path d="M33 28 C22 40, 22 60, 33 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
  <path d="M44 24 C32 40, 32 60, 44 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
  <path d="M56 24 C44 40, 44 60, 56 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
  <path d="M67 28 C56 40, 56 60, 67 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
  <circle cx="40" cy="40" r="3" fill="#FFE3E3"/>
  <circle cx="44" cy="55" r="3.5" fill="#9FC3E3"/>
  <circle cx="50" cy="70" r="3" fill="#FFE3E3"/>
  <circle cx="52" cy="38" r="3.5" fill="#9FC3E3"/>
  <circle cx="58" cy="53" r="3" fill="#FFE3E3"/>
  <circle cx="62" cy="68" r="3.5" fill="#9FC3E3"/>
  <path d="M46 16 C48 10, 52 6, 58 8" stroke="#E8B4B4" stroke-width="3" fill="none" stroke-linecap="round"/>
  <path d="M58 8 C62 6, 64 8, 66 7" stroke="#E8B4B4" stroke-width="2.5" fill="none" stroke-linecap="round"/>
</svg>
```

- [ ] **Step 4: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检：标题栏为白底红 Logo，最小化/最大化 hover 红色浅染，关闭 hover 红底白字。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/TitleBar.svelte frontend/public/keinestamp.svg
git commit -m "feat(ui): 标题栏与 Logo 换红白配色"
```

---

### Task 3: ProjectTabs 胶囊标签样式

**Files:**
- Modify: `frontend/src/lib/ProjectTabs.svelte`（`<style>` 整块替换，第 52-117 行）

**Interfaces:**
- Consumes: Task 1 令牌
- Produces: 无（模板与 props 不变）

- [ ] **Step 1: 替换 `<style>` 整块**

```css
  .project-tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    min-height: 36px;
  }
  .project-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 12px;
    font-size: 11px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 999px;
    cursor: pointer;
    color: var(--text-dim);
    white-space: nowrap;
    max-width: 160px;
    transition: background 0.15s var(--ease), color 0.15s var(--ease),
      border-color 0.15s var(--ease), transform 0.15s var(--ease);
  }
  .project-tab:hover {
    background: var(--surface);
    border-color: var(--border);
    color: var(--text);
    transform: translateY(-1px);
  }
  .project-tab.active {
    background: var(--surface);
    border-color: var(--border);
    color: var(--accent);
    font-weight: 600;
    box-shadow: var(--shadow-sm);
  }
  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add-tab, .settings-tab {
    padding: 4px 8px;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
  }
  .add-tab:hover, .settings-tab:hover {
    background: var(--surface);
    border-color: var(--border);
    color: var(--accent);
    transform: none;
  }
  .tabs-spacer { flex: 1; }
```

- [ ] **Step 2: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检：标签为白色胶囊，选中态红字白底微阴影，hover 上移。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/ProjectTabs.svelte
git commit -m "feat(ui): 项目标签改为胶囊风格"
```

---

### Task 4: 布局微调 — 侧栏折叠 + 时间线拖拽调宽 + 工具栏收编

**Files:**
- Modify: `frontend/src/App.svelte`
- Modify: `frontend/src/lib/Sidebar.svelte`

**Interfaces:**
- Consumes: Task 1 的 `.sidebar.collapsed`（宽度 44px）与 `--sidebar-w-collapsed`
- Produces:
  - `App.svelte` 状态：`sidebarCollapsed: boolean`、`timelineWidth: number`（持久化 localStorage key `keinestamp_sidebar_collapsed` / `keinestamp_timeline_width`）
  - `Sidebar.svelte` 新 props：`collapsed: boolean`、`onExpand: () => void`
  - `App.svelte` 内 `.toolbar-icon`（工具栏图标按钮）与 `.tl-resize`（拖拽手柄）样式

- [ ] **Step 1: App.svelte 脚本区新增状态与逻辑（插入到 `let deploying = $state(false)` 之后，第 45 行）**

```ts
  let sidebarCollapsed = $state(localStorage.getItem('keinestamp_sidebar_collapsed') === '1')
  let timelineWidth = $state(parseInt(localStorage.getItem('keinestamp_timeline_width') || '320', 10))

  $effect(() => {
    localStorage.setItem('keinestamp_sidebar_collapsed', sidebarCollapsed ? '1' : '0')
  })
  $effect(() => {
    localStorage.setItem('keinestamp_timeline_width', String(timelineWidth))
  })

  function startTimelineDrag(e: PointerEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startW = timelineWidth
    const onMove = (ev: PointerEvent) => {
      const w = startW - (ev.clientX - startX)
      timelineWidth = Math.min(480, Math.max(240, w))
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }
```

- [ ] **Step 2: App.svelte 模板改动**

把 `<Sidebar {trackedFiles} {activePath} onCloseProject={handleCloseAndRemove} />` 改为：

```svelte
    <Sidebar
      {trackedFiles}
      {activePath}
      collapsed={sidebarCollapsed}
      onExpand={() => sidebarCollapsed = false}
      onCloseProject={handleCloseAndRemove}
    />
```

在工具栏内（`.toolbar` 开头，第 290-293 行之间）加折叠按钮：

```svelte
      <div class="toolbar">
        <button
          class="toolbar-icon"
          onclick={() => sidebarCollapsed = !sidebarCollapsed}
          title={sidebarCollapsed ? '展开侧栏' : '折叠侧栏'}
        >
          {#if sidebarCollapsed}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M6 3l5 5-5 5V3z"/></svg>
          {:else}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M3 6l5 5 5-5H3z"/></svg>
          {/if}
        </button>
```

把 `<div class="timeline-panel">`（第 324 行）改为：

```svelte
        <div class="tl-resize" onpointerdown={startTimelineDrag}></div>
        <div class="timeline-panel" style="width: {timelineWidth}px">
```

（删除原 `.timeline-panel { width: 320px; ... }` 中的 `width: 320px;`，见 Step 3。）

- [ ] **Step 3: App.svelte `<style>` 调整**

`.timeline-panel` 规则改为（删除固定宽度）：

```css
  .timeline-panel {
    border-left: 1px solid var(--border);
    background: var(--bg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    transition: none;
  }
```

在 `.toolbar-path` 规则前新增：

```css
  .toolbar-icon {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 4px 6px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: background 0.12s, color 0.12s;
  }
  .toolbar-icon:hover {
    background: var(--surface2);
    color: var(--accent);
  }
  .tl-resize {
    width: 5px;
    cursor: col-resize;
    flex-shrink: 0;
    background: transparent;
    transition: background 0.15s var(--ease);
    z-index: 1;
  }
  .tl-resize:hover {
    background: var(--accent-soft);
  }
```

`.path-folder:hover` 规则中的 `color: var(--accent, #cba6f7);` 与 `background: rgba(203, 166, 247, 0.12);` 改为 `color: var(--accent); background: var(--accent-soft);`。

- [ ] **Step 4: Sidebar.svelte 支持折叠**

props 定义（第 4-12 行）改为：

```ts
  let {
    trackedFiles = [],
    activePath = '',
    collapsed = false,
    onExpand = () => {},
    onCloseProject = (_p: string) => {},
  }: {
    trackedFiles?: string[]
    activePath?: string
    collapsed?: boolean
    onExpand?: () => void
    onCloseProject: (path: string) => void
  } = $props()
```

根元素（第 23 行）改为：

```svelte
<div class="sidebar" class:collapsed>
  {#if collapsed}
    <div class="sidebar-collapsed-bar">
      <button class="expand-btn" onclick={onExpand} title="展开侧栏">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M6 3l5 5-5 5V3z"/></svg>
      </button>
    </div>
  {:else}
    <div class="sidebar-header">
```

并在原 `.sidebar-bottom` 闭合 `</div>`（第 55 行）后追加：

```svelte
  {/if}
</div>
```

（即：header/list/bottom 三块整体包进 `{:else} ... {/if}`，外层根 div 保留 `class:collapsed`。）

Sidebar `<style>` 追加：

```css
  .sidebar-collapsed-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 10px;
  }
  .expand-btn {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 6px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .expand-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
```

并把 `.close-btn:hover` 规则改为：

```css
  .close-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
```

- [ ] **Step 5: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误（Svelte 5 中 `{#if}` 包裹需保持标签配对正确，svelte-check 会报错可及时发现）。目检清单：
1. 工具栏左端有折叠按钮，点击后侧栏缩为 44px 窄条（含展开箭头），再点恢复
2. 刷新页面后折叠状态保留（localStorage）
3. 时间线面板左缘出现拖拽手柄，拖拽可 240–480px 调宽，刷新后宽度保留
4. 折叠时退出文件夹按钮、文件树均隐藏

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.svelte frontend/src/lib/Sidebar.svelte
git commit -m "feat(ui): 侧栏折叠与时间线拖拽调宽"
```

---

### Task 5: StatusPanel 样式细化

**Files:**
- Modify: `frontend/src/lib/StatusPanel.svelte`（`<style>` 块内若干规则）

**Interfaces:**
- Consumes: Task 1 令牌
- Produces: 无

- [ ] **Step 1: 替换 `<style>` 整块（第 105-188 行）**

```css
  .status-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .sp-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .sp-title {
    font-size: 13px;
    font-weight: 600;
  }
  .sp-count {
    font-size: 11px;
    color: var(--text-dim);
    background: var(--surface2);
    padding: 1px 8px;
    border-radius: 999px;
  }
  .sp-spacer { flex: 1; }
  .sp-refresh {
    background: none;
    border: none;
    font-size: 16px;
    padding: 2px 6px;
    cursor: pointer;
    color: var(--text-dim);
    border-radius: 6px;
    transition: background 0.12s, color 0.12s;
  }
  .sp-refresh:hover { background: var(--surface2); color: var(--accent); }
  .sp-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    gap: 8px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .sp-empty-icon { font-size: 28px; opacity: 0.5; }
  .sp-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 8px;
  }
  .sp-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    margin-bottom: 2px;
    font-size: 13px;
    border-radius: 8px;
    transition: background 0.12s var(--ease), box-shadow 0.12s var(--ease);
  }
  .sp-item:hover { background: var(--surface); box-shadow: var(--shadow-sm); }
  .sp-icon { font-size: 12px; width: 18px; text-align: center; flex-shrink: 0; }
  .sp-ext-badge {
    font-size: 9px;
    font-weight: 600;
    background: var(--ice-soft);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--blue);
    flex-shrink: 0;
    letter-spacing: 0.3px;
  }
  .sp-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  .sp-label {
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    flex-shrink: 0;
  }
```

- [ ] **Step 2: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检：变更行 hover 白底上浮微阴影，扩展名徽章冰蓝底深蓝字，刷新按钮 hover 变红。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/StatusPanel.svelte
git commit -m "feat(ui): Changes 面板样式细化"
```

---

### Task 6: EntryList 时间线卡片与 diff 配色

**Files:**
- Modify: `frontend/src/lib/EntryList.svelte`

**Interfaces:**
- Consumes: Task 1 令牌
- Produces: 组件内 `tagSoft(tag: string): string` 辅助函数（浅染背景色，供标签徽章内联样式用）

- [ ] **Step 1: 替换 TAG_COLORS / FALLBACK_COLORS 与新增 tagSoft（第 118-132 行）**

```ts
  const TAG_COLORS: Record<string, string> = {
    '建模': '#3d6fa8',
    'UV拆分': '#3e6b52',
    '绑定': '#7a5fa8',
    '贴图': '#a8722e',
    '修复': '#c03535',
  }
  const FALLBACK_COLORS = ['#3d6fa8', '#3e6b52', '#7a5fa8', '#a8722e', '#c03535', '#4e8a8a']

  function tagColor(tag: string): string {
    if (TAG_COLORS[tag]) return TAG_COLORS[tag]
    let hash = 0
    for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i)
    return FALLBACK_COLORS[Math.abs(hash) % FALLBACK_COLORS.length]
  }

  function tagSoft(tag: string): string {
    const c = tagColor(tag)
    const r = parseInt(c.slice(1, 3), 16)
    const g = parseInt(c.slice(3, 5), 16)
    const b = parseInt(c.slice(5, 7), 16)
    return `rgba(${r},${g},${b},0.12)`
  }
```

- [ ] **Step 2: 标签徽章内联样式（第 264-266 行）**

```svelte
          {#if entryTag}
            <span class="tag-badge" style="color:{tagColor(entryTag)};background:{tagSoft(entryTag)}">{entryTag}</span>
          {/if}
```

- [ ] **Step 3: 替换 `<style>` 整块（第 385-578 行）**

```css
  .section-header {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 6px;
    padding: 5px 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    border-top: 1px solid var(--border);
    transition: color 0.12s;
  }
  .section-header:hover { color: var(--accent); }
  .section-header .arr { font-size: 9px; width: 10px; flex-shrink: 0; }
  .section-body { padding: 4px 0 2px 15px; font-size: 12px; }
  .section-loading { color: var(--text-dim); font-size: 11px; padding: 4px 0; }
  .file-status-line { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-family: var(--mono); font-size: 12px; }
  .status-badge {
    width: 18px; height: 18px;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 999px;
    font-weight: 700; font-size: 12px;
    flex-shrink: 0;
  }
  .status-path { color: var(--text); }
  .st-add { color: var(--ze); background: var(--ze-soft); }
  .st-del { color: var(--error); background: var(--accent-soft); }
  .st-mod { color: var(--warning); background: rgba(176, 128, 58, 0.12); }

  .diff-entry, .model-entry { margin-bottom: 6px; }
  .diff-path {
    font-size: 11px; font-family: var(--mono);
    color: var(--blue); padding: 2px 6px;
    background: var(--blue-soft); border-radius: 4px; margin-bottom: 3px;
  }
  .diff-empty { font-size: 11px; color: var(--text-dim); padding: 2px 6px; }

  .dl {
    display: flex;
    gap: 8px;
    font-family: var(--mono);
    font-size: 11px;
    line-height: 1.6;
    padding: 0 4px;
    border-radius: 3px;
  }
  .dl-num {
    width: 32px;
    text-align: right;
    flex-shrink: 0;
    color: var(--text-dim);
    font-size: 10px;
    user-select: none;
  }
  .dl-text { white-space: pre; flex: 1; }
  .dl.add { background: var(--ze-soft); color: var(--ze); }
  .dl.add .dl-num { color: var(--ze); }
  .dl.del { background: var(--accent-soft); color: var(--error); }
  .dl.del .dl-num { color: var(--error); }
  .dl.mod { background: var(--blue-soft); color: var(--blue); }
  .dl.mod .dl-num { color: var(--blue); }

  .model-err { font-size: 11px; color: var(--text-dim); padding: 8px; text-align: center; background: var(--surface2); border-radius: 6px; }
  .open-web-btn {
    margin-top: 6px;
    padding: 5px 14px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .open-web-btn:hover {
    background: var(--accent);
    color: #fff;
  }
  .tag-group { display: flex; align-items: center; gap: 8px; margin: 12px 0 6px; }
  .tag-group-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; box-shadow: 0 0 0 3px var(--surface); }
  .tag-group-label { font-size: 12px; font-weight: 700; }
  .tag-group-line { flex: 1; height: 1px; background: var(--border); }
  .tag-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    margin-left: 6px;
    vertical-align: middle;
  }
  .entries-footer {
    padding: 10px 0 4px;
    border-top: 1px solid var(--border);
    margin-top: 8px;
  }
  .at-section label {
    display: block;
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .at-row {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .at-input {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    font-family: var(--mono);
  }
  .at-masked {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    transition: background 0.12s, border-color 0.12s;
    font-family: var(--mono);
  }
  .at-masked:hover {
    background: var(--surface2);
    border-color: var(--accent);
  }
  .at-masked.copied {
    color: var(--ze);
    border-color: var(--ze);
  }
  .at-empty {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    text-align: center;
    transition: background 0.12s, border-color 0.12s, color 0.12s;
  }
  .at-empty:hover {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .at-gen-btn {
    font-size: 10px;
    padding: 3px 10px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px dashed var(--accent);
    color: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }
  .at-gen-btn:hover {
    background: var(--accent-soft);
  }
  .at-save-btn {
    font-size: 10px;
    padding: 3px 10px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }
  .at-save-btn:hover:not(:disabled) {
    background: var(--surface2);
  }
  .at-save-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .at-save-btn.saved {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
```

- [ ] **Step 4: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检清单：
1. 卡片白底圆角，hover 上浮 + 红边框，选中红色左边条 + 红边框
2. 标签徽章为浅染底 + 深色文字胶囊
3. 文件状态徽章圆形浅染（+墨绿 / −红 / ~暖金）
4. diff 行 add 墨绿浅染 / del 红浅染 / mod 深蓝浅染
5. 「3D 模型预览」红色描边按钮，hover 红底白字
6. 访问令牌区交互样式正常

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/EntryList.svelte
git commit -m "feat(ui): 时间线卡片与 diff 配色重做"
```

---

### Task 7: ActionBar 按钮层级

**Files:**
- Modify: `frontend/src/lib/ActionBar.svelte`

**Interfaces:**
- Consumes: Task 1 令牌
- Produces: 记录按钮加 `record-btn` 类

- [ ] **Step 1: 记录按钮加类（第 24 行）**

```svelte
  <button class="record-btn" onclick={onRecord} disabled={!initialized}>
```

- [ ] **Step 2: 替换 `<style>` 整块（第 47-94 行）**

```css
  .action-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: var(--action-h);
    padding: 0 10px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .action-bar button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    font-size: 11px;
    padding: 4px 16px;
    border-radius: 8px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    transition: background 0.15s var(--ease), border-color 0.15s var(--ease),
      box-shadow 0.15s var(--ease);
  }
  .action-bar button:hover:not(:disabled) {
    background: var(--surface2);
    border-color: var(--surface3);
  }
  .action-bar button:disabled {
    opacity: 0.35;
    cursor: default;
    pointer-events: none;
  }
  .ab-label {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.2;
  }
  .ab-hint {
    font-size: 9px;
    color: var(--text-dim);
    line-height: 1.1;
  }
  .record-btn {
    background: var(--accent) !important;
    border-color: var(--accent) !important;
    color: #fff !important;
  }
  .record-btn .ab-hint {
    color: rgba(255, 255, 255, 0.75);
  }
  .record-btn:hover:not(:disabled) {
    background: var(--accent-hover) !important;
    border-color: var(--accent-hover) !important;
    box-shadow: 0 2px 8px rgba(217, 71, 71, 0.35);
  }
  .deploy-btn {
    border-color: var(--ze) !important;
    color: var(--ze) !important;
    background: transparent !important;
  }
  .deploy-btn:hover:not(:disabled) {
    background: var(--ze-soft) !important;
    border-color: var(--ze) !important;
    color: var(--ze-hover) !important;
  }
  .deploy-btn .ab-hint {
    color: var(--ze);
    opacity: 0.75;
  }
  .spacer { flex: 1; }
```

- [ ] **Step 3: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检：记录为红底白字主按钮，部署为墨绿描边，导出/撤回白底描边。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/ActionBar.svelte
git commit -m "feat(ui): 操作栏按钮层级重做"
```

---

### Task 8: 弹窗统一（CommitDialog / SettingsDialog / DetailPanel）

**Files:**
- Modify: `frontend/src/lib/CommitDialog.svelte`（`<style>` 中 `.tag-btn` 相关规则）
- Modify: `frontend/src/lib/SettingsDialog.svelte`（`<style>` 中 overlay/panel/progress/status 规则）
- Modify: `frontend/src/lib/DetailPanel.svelte`（tag-badge/blend-item/open-web-btn/open-err + 新增 tagSoft）

**Interfaces:**
- Consumes: Task 1 的 `.dialog-overlay`/`.dialog`、`.detail-overlay`/`.detail-panel` 全局样式与 keyframes
- Produces: 无

- [ ] **Step 1: CommitDialog — 替换 `.tag-btn` 相关规则（第 109-131 行）**

```css
  .tag-btn {
    font-size: 12px;
    padding: 4px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    transition: all 0.15s var(--ease);
  }
  .tag-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
    font-weight: 600;
  }
  .custom-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
```

- [ ] **Step 2: SettingsDialog — 替换样式块内的 5 处规则（第 236-252 行 overlay/panel、第 274-276 行 progress-bar、第 295-297 行 dl-status、第 305-307 行 conn-result）**

```css
  .overlay {
    position: fixed; inset: 0;
    background: rgba(30, 40, 51, 0.35);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade-in 0.15s var(--ease);
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 24px;
    width: 520px;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: var(--shadow-lg);
    animation: dialog-in 0.2s var(--ease);
  }
```

```css
  .progress-bar {
    height: 100%; background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }
```

```css
  .dl-status { font-size: 11px; color: var(--text-dim); margin-top: 4px; display: block; }
  .dl-status.success { color: var(--ze); }
  .dl-status.error { color: var(--error); }
```

```css
  .conn-result { display: block; font-size: 11px; margin-top: 4px; }
  .conn-result.ok { color: var(--ze); }
  .conn-result.fail { color: var(--error); }
```

- [ ] **Step 3: DetailPanel — 脚本区新增 tagSoft（插在 tagColor 函数后，第 19 行后）**

```ts
  function tagSoft(tag: string): string {
    const c = tagColor(tag)
    const r = parseInt(c.slice(1, 3), 16)
    const g = parseInt(c.slice(3, 5), 16)
    const b = parseInt(c.slice(5, 7), 16)
    return `rgba(${r},${g},${b},0.12)`
  }
```

- [ ] **Step 4: DetailPanel — TAG_COLORS/FALLBACK 换新（第 6-13 行）**

```ts
  const TAG_COLORS: Record<string, string> = {
    '建模': '#3d6fa8',
    'UV拆分': '#3e6b52',
    '绑定': '#7a5fa8',
    '贴图': '#a8722e',
    '修复': '#c03535',
  }
  const FALLBACK_COLORS = ['#3d6fa8', '#3e6b52', '#7a5fa8', '#a8722e', '#c03535', '#4e8a8a']
```

- [ ] **Step 5: DetailPanel — 标签徽章内联样式（第 54 行）**

```svelte
          <span class="tag-badge" style="color:{tagColor(entry.tag)};background:{tagSoft(entry.tag)}">{entry.tag}</span>
```

- [ ] **Step 6: DetailPanel — 替换 `<style>` 整块（第 95-125 行）**

```css
  .ft { max-height: 200px; overflow-y: auto; }
  .tag-badge {
    display: inline-block;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 10px;
    border-radius: 999px;
    margin-left: 8px;
    vertical-align: middle;
  }
  .blend-item { font-family: var(--mono); font-size: 11px; color: var(--blue); margin-bottom: 4px; }
  .open-web-btn {
    margin-top: 8px;
    padding: 6px 16px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .open-web-btn:hover {
    background: var(--accent);
    color: #fff;
  }
  .open-err { color: var(--error); font-size: 11px; margin-top: 4px; }
```

- [ ] **Step 7: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检清单：
1. 记录弹窗遮罩有毛玻璃模糊，弹窗 fade+scale 入场；标签胶囊选中为红字浅红底
2. 设置弹窗同款入场，下载进度条为红色，成功/失败文字为墨绿/红
3. 详情弹窗标签徽章浅染底，blend 路径为深蓝，打开按钮红描边

- [ ] **Step 8: Commit**

```bash
git add frontend/src/lib/CommitDialog.svelte frontend/src/lib/SettingsDialog.svelte frontend/src/lib/DetailPanel.svelte
git commit -m "feat(ui): 弹窗样式统一（毛玻璃遮罩 + 红色主视觉）"
```

---

### Task 9: Toast 图标化

**Files:**
- Modify: `frontend/src/lib/Toast.svelte`（模板 + 新增 `<style>`）

**Interfaces:**
- Consumes: Task 1 的 `.toast-container/.toast` 全局样式
- Produces: 无（props 不变）

- [ ] **Step 1: 替换 Toast.svelte 整个文件内容（11 行 → 新模板 + 样式）**

```svelte
<script lang="ts">
  let { toasts }: { toasts: { id: number; msg: string; type: string }[] } = $props()
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as t (t.id)}
      <div class="toast {t.type}">
        <span class="toast-icon">
          {#if t.type === 'success'}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M13.854 3.646a.5.5 0 010 .708l-7 7a.5.5 0 01-.708 0l-3.5-3.5a.5.5 0 11.708-.708L6.5 10.293l6.646-6.647a.5.5 0 01.708 0z"/></svg>
          {:else if t.type === 'error'}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 1a7 7 0 107 7 7 7 0 00-7-7zm3.354 9.646a.5.5 0 01-.708.708L8 8.707l-2.646 2.647a.5.5 0 01-.708-.708L7.293 8 4.646 5.354a.5.5 0 01.708-.708L8 7.293l2.646-2.647a.5.5 0 01.708.708L8.707 8z"/></svg>
          {:else}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 15A7 7 0 118 1a7 7 0 010 14zm0 1A8 8 0 108 0a8 8 0 000 16z"/><path d="M8 4a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 018 4zm0 7.5a.75.75 0 110 1.5.75.75 0 010-1.5z"/></svg>
          {/if}
        </span>
        <span class="toast-msg">{t.msg}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }
  .toast.success .toast-icon { color: var(--ze); }
  .toast.error .toast-icon { color: var(--error); }
  .toast.info .toast-icon { color: var(--ice); }
  .toast-msg { flex: 1; }
</style>
```

- [ ] **Step 2: 运行检查 + 目检**

Run: `cd frontend; npm run check`
Expected: 无错误。目检：触发成功/错误/信息 toast（记录、部署、revert 等），确认白底左侧色条 + 同色图标 + 滑入动画。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/Toast.svelte
git commit -m "feat(ui): Toast 图标化"
```

---

### Task 10: 收尾核对与全局清理

**Files:**
- Verify: 所有 Task 1-9 涉及文件
- Modify: 若 grep 发现残留旧色值则修复

**Interfaces:**
- Consumes: 全部任务产物

- [ ] **Step 1: 搜索残留旧色值**

Run: `cd frontend; rg -n "cba6f7|1e1e2e|4caf88|e06c75|e5c07b|648cff|4ade80|f87171|7c3aed|388E3C|4CAF50|66BB6A|8BC34A" src/`
Expected: 无输出。若有残留，逐个替换为对应令牌后重跑 `npm run check`。

- [ ] **Step 2: 完整验证**

Run: `cd frontend; npm run check`
Expected: 无错误。
Run: `cargo test`（确保 Rust 侧不受影响 — 本计划无 Rust 改动，应原样通过）
Expected: 70 unit + 7 integration 全过。

- [ ] **Step 3: 全量视觉目检清单**

1. 标题栏红白 Logo、窗口控制 hover 正常
2. 标签页胶囊红字选中态
3. 侧栏折叠/展开 + 文件树 hover 与选中红边
4. Changes 面板行 hover 上浮、扩展名冰蓝徽章
5. 时间线卡片 hover 上浮红框、标签浅染徽章、状态圆形徽章、diff 三色浅染
6. 操作栏记录红底白字 / 部署墨绿描边
7. 三个弹窗毛玻璃遮罩 + fade/scale 入场
8. Toast 白底 + 色条 + 图标
9. 空状态冰蓝圆形图标底（「从上方标签页选择一个项目」页）
10. 滚动条细条样式

- [ ] **Step 4: Commit（如有残留修复）**

```bash
git add frontend/src
git commit -m "fix(ui): 清理残留旧色值"
```
（若 Step 1 无输出则跳过此提交）

---

## Self-Review 结论（已执行）

- **Spec 覆盖**：设计令牌/视觉语言 → Task 1；Logo → Task 2；标签页 → Task 3；侧栏折叠 + 时间线拖拽 + 工具栏收编 → Task 4；StatusPanel → Task 5；EntryList → Task 6；ActionBar → Task 7；弹窗三件套 → Task 8；Toast → Task 9；空状态/滚动条 → Task 1（app.scss 全局）；不做的（无 Rust 改动/无新依赖/无暗色模式）→ Global Constraints。
- **占位符扫描**：无 TBD/TODO；所有样式块均为完整可粘贴代码。
- **类型一致性**：`tagSoft` 在 Task 6/8 定义一致（返回 `rgba()` 字符串）；`Sidebar.svelte` props `collapsed/onExpand` 与 App.svelte 传入一致；`startTimelineDrag` 在 App.svelte 内定义并使用。
