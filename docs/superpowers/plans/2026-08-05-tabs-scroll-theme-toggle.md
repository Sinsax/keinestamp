# 标签栏滚动修复 + 主题按钮迁移 + 美化实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复仓库标签过多时溢出覆盖设置按钮的问题（滚动标签区 + 滚轮水平滚动），把日夜主题切换按钮从 TitleBar 移到 ProjectTabs 设置按钮旁，并做少量美化（标签 hover 关闭 ×、连接点颜色收尾、滚动区渐隐）。

**Architecture:** ProjectTabs 结构拆为 `.tabs-scroll`（flex:1 + overflow-x:auto 的可滚动标签区）与 `.tabs-actions`（固定右侧 ☀/🌙 + ⚙ 按钮区，细分隔线）。滚轮水平滚动用 Svelte action 绑定 `wheel`（`{ passive: false }`）把 `deltaY` 转为 `scrollLeft`；激活标签 `scrollIntoView` 自动定位；可滚动时才加 `mask-image` 两端渐隐与悬停滚动条。主题状态 `theme`/`onToggleTheme` 从 TitleBar 迁移到 ProjectTabs（App.svelte 统一持有状态）。标签 hover 关闭 × 复用 App.handleRemoveRecent（confirm 后移除）。

**Tech Stack:** Svelte 5（`$props`/`$state`/`$effect`/action）、现有 SCSS 令牌体系。无 Rust 改动。

## Global Constraints

- 验证命令：`cd frontend; npm run check`（0 errors）
- 不引入新依赖；纯前端改动，无 Rust 文件变更
- 图标约定沿用 TitleBar 现状：日间显示太阳、夜间显示月亮（表示当前模式），tooltip 提示切换动作
- localStorage 键 `keinestamp_theme` 与日夜色板逻辑不变（本次只移动按钮位置）
- 根目录 `package-lock.json` 永不创建/提交
- 提交消息格式：`feat(tabs): ...` / `fix(ui): ...` / `docs: ...`
- 标签样式保留现有胶囊风格与 `max-width: 160px` 省略号，不改宽度策略

---

### Task 1: ProjectTabs 重构（滚动区 + 主题按钮 + 关闭 ×）

**Files:**
- Modify: `frontend/src/lib/ProjectTabs.svelte`

**Interfaces:**
- Consumes: App.svelte 现有 `theme` 状态（后续 Task 2 接线）
- Produces:
  - 新 props：`theme: 'day' | 'night'`（默认 `'day'`）、`onToggleTheme: () => void`（默认空）、`onRemoveRecent?: (path: string) => void`（默认空）
  - 模板结构：`.project-tabs` → `.tabs-scroll`（含标签 + add 按钮）+ `.tabs-actions`（☀/🌙 + ⚙）
  - 滚动 action：`use:wheelScroll`（定义于本组件内），wheel → scrollLeft；`$effect` 监听 `activePath` 自动滚动激活标签入视野；可滚动检测给 `.tabs-scroll` 加 `scrollable` 类

- [ ] **Step 1: 脚本区重写（props + action + effects）**

```ts
<script lang="ts">
  import type { RecentEntry } from '../types'

  let {
    entries,
    activePath,
    onSelect,
    onNewProject,
    onSettings,
    theme = 'day',
    onToggleTheme = () => {},
    onRemoveRecent = (_p: string) => {},
  }: {
    entries: RecentEntry[]
    activePath: string
    onSelect: (path: string) => void
    onNewProject: () => void
    onSettings: () => void
    theme?: string
    onToggleTheme?: () => void
    onRemoveRecent?: (path: string) => void
  } = $props()

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  function projectName(p: string): string {
    return cleanPath(p).split(/[/\\]/).pop() || cleanPath(p)
  }

  let scrollEl = $state<HTMLElement | null>(null)

  function wheelScroll(node: HTMLElement) {
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return
      if (node.scrollWidth <= node.clientWidth) return
      e.preventDefault()
      node.scrollLeft += e.deltaY
    }
    node.addEventListener('wheel', onWheel, { passive: false })
    return {
      destroy() {
        node.removeEventListener('wheel', onWheel)
      },
    }
  }

  function handleTabClose(e: MouseEvent, path: string) {
    e.stopPropagation()
    onRemoveRecent(path)
  }

  let isScrollable = $state(false)

  $effect(() => {
    const el = scrollEl
    if (!el) return
    const check = () => { isScrollable = el.scrollWidth > el.clientWidth }
    check()
    const ro = new ResizeObserver(check)
    ro.observe(el)
    return () => ro.disconnect()
  })

  $effect(() => {
    activePath
    if (!scrollEl) return
    const active = scrollEl.querySelector<HTMLElement>('.project-tab.active')
    active?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  })
</script>
```

- [ ] **Step 2: 模板重写**

```svelte
<div class="project-tabs">
  <div class="tabs-scroll" class:scrollable={isScrollable} use:wheelScroll bind:this={scrollEl}>
    {#each entries as e}
      <button
        class="project-tab"
        class:active={e.path === activePath}
        onclick={() => onSelect(e.path)}
        title={cleanPath(e.path)}
      >
        <span class="tab-name">{projectName(e.path)}</span>
        {#if e.path === activePath}
          <span class="tab-close" onclick={(ev) => handleTabClose(ev, e.path)} title="移除仓库">
            <svg viewBox="0 0 12 12" width="9" height="9" fill="currentColor"><path d="M2.646 2.646a.5.5 0 0 1 .708 0L6 5.293l2.646-2.647a.5.5 0 0 1 .708.708L6.707 6l2.647 2.646a.5.5 0 0 1-.708.708L6 6.707l-2.646 2.647a.5.5 0 0 1-.708-.708L5.293 6 2.646 3.354a.5.5 0 0 1 0-.708z"/></svg>
          </span>
        {/if}
      </button>
    {/each}
    <button class="project-tab add-tab" onclick={onNewProject} title="New project">
      <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
        <path d="M8 2a.5.5 0 0 1 .5.5v5h5a.5.5 0 0 1 0 1h-5v5a.5.5 0 0 1-1 0v-5h-5a.5.5 0 0 1 0-1h5v-5A.5.5 0 0 1 8 2z"/>
      </svg>
    </button>
  </div>
  <div class="tabs-actions">
    <button class="project-tab theme-tab" onclick={onToggleTheme} title={theme === 'night' ? '切换到日间模式' : '切换到夜间模式'}>
      {#if theme === 'night'}
        <svg viewBox="0 0 12 12" width="11" height="11" fill="currentColor"><path d="M6 1a5 5 0 100 10A5 5 0 016 1z"/></svg>
      {:else}
        <svg viewBox="0 0 12 12" width="11" height="11" fill="currentColor"><path d="M6 3a3 3 0 100 6 3 3 0 000-6zm0-2a.5.5 0 01.5.5v1a.5.5 0 01-1 0v-1A.5.5 0 016 1zm0 7a.5.5 0 01.5.5v1a.5.5 0 01-1 0v-1A.5.5 0 016 8zm4-2a.5.5 0 010 1H9a.5.5 0 010-1h1zM2 6a.5.5 0 01.5-.5H4a.5.5 0 010 1H2.5A.5.5 0 012 6zm3.646-3.354a.5.5 0 01.708 0l.707.707a.5.5 0 01-.707.708l-.708-.707a.5.5 0 010-.708zm3.354 3.354a.5.5 0 01.708 0l.707.707a.5.5 0 01-.708.708l-.707-.708a.5.5 0 010-.707z"/></svg>
      {/if}
    </button>
    <button class="project-tab settings-tab" onclick={onSettings} title="Settings">
      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
        <path d="M8 4.754a3.246 3.246 0 100 6.492 3.246 3.246 0 000-6.492zM5.754 8a2.246 2.246 0 114.492 0 2.246 2.246 0 01-4.492 0z"/>
        <path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 01-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 01-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 01.52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 011.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 011.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 01.52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 01-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 01-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 002.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 001.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 00-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 00-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 00-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 001.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 003.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 002.692-1.115l.094-.319z"/>
      </svg>
    </button>
  </div>
</div>
```

- [ ] **Step 3: `<style>` 整块替换**

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
  .tabs-scroll {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    scrollbar-color: transparent transparent;
  }
  .project-tabs:hover .tabs-scroll {
    scrollbar-color: var(--surface3) transparent;
  }
  .tabs-scroll::-webkit-scrollbar { height: 4px; }
  .tabs-scroll::-webkit-scrollbar-track { background: transparent; }
  .tabs-scroll::-webkit-scrollbar-thumb {
    background: transparent;
    border-radius: 2px;
  }
  .project-tabs:hover .tabs-scroll::-webkit-scrollbar-thumb {
    background: var(--surface3);
  }
  .tabs-scroll.scrollable {
    mask-image: linear-gradient(to right, transparent 0, #000 10px, #000 calc(100% - 10px), transparent 100%);
    -webkit-mask-image: linear-gradient(to right, transparent 0, #000 10px, #000 calc(100% - 10px), transparent 100%);
  }
  .tabs-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    margin-left: 6px;
    padding-left: 6px;
    border-left: 1px solid var(--border);
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
  .tab-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    color: var(--text-dim);
    flex-shrink: 0;
    transition: background 0.12s, color 0.12s;
  }
  .tab-close:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .add-tab, .settings-tab, .theme-tab {
    padding: 4px 8px;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .add-tab:hover, .settings-tab:hover, .theme-tab:hover {
    background: var(--surface);
    border-color: var(--border);
    color: var(--accent);
    transform: none;
  }
```

- [ ] **Step 4: 验证**

Run: `cd frontend; npm run check`
Expected: 0 errors。目检（若可行）：12+ 仓库时标签区可滚动、滚轮水平滑动、激活标签自动滚入视野、悬停细滚动条浮现、两端渐隐、☀/🌙 与 ⚙ 固定在右侧、激活标签显示 ×。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/ProjectTabs.svelte
git commit -m "feat(tabs): 标签区滚动与右侧固定按钮区（含主题/关闭/渐隐）"
```

---

### Task 2: TitleBar 还原 + App 接线

**Files:**
- Modify: `frontend/src/lib/TitleBar.svelte`
- Modify: `frontend/src/App.svelte`

**Interfaces:**
- Consumes: Task 1 的 ProjectTabs 新 props（`theme`、`onToggleTheme`、`onRemoveRecent`）
- Produces: TitleBar 无 props（纯窗口控制）；App.svelte 向 ProjectTabs 传 `{theme}`、`onToggleTheme`、`onRemoveRecent={handleRemoveRecent}`

- [ ] **Step 1: TitleBar.svelte 移除主题 props 与按钮**

- 脚本顶部删除 `$props()` 解构块（`theme = 'day'`、`onToggleTheme = () => {}`），保留 `win` 相关代码
- 模板删除 `.tb-spacer` 与 `.tb-controls` 之间的主题按钮（含 `onclick={onToggleTheme}` 与两个 SVG 的整个 button 块）
- `<style>` 不变（`.tb-btn` 等仍被窗口控制按钮使用）

- [ ] **Step 2: App.svelte 接线**

`<TitleBar theme={theme} onToggleTheme={...} />` 改为 `<TitleBar />`。

`<ProjectTabs` 调用处追加 props：

```svelte
  <ProjectTabs
    entries={recentProjects.toReversed()}
    activePath={activePath}
    onSelect={selectRecent}
    onNewProject={handleNewProject}
    onSettings={() => showSettings = true}
    theme={theme}
    onToggleTheme={() => theme = theme === 'day' ? 'night' : 'day'}
    onRemoveRecent={handleRemoveRecent}
  />
```
（现有 `handleRemoveRecent(recentPath)` 已带 confirm 确认框，直接复用。）

- [ ] **Step 3: 验证**

Run: `cd frontend; npm run check`
Expected: 0 errors（svelte-check 校验 props 类型）。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/TitleBar.svelte frontend/src/App.svelte
git commit -m "feat(tabs): 主题按钮移至标签栏右侧，TitleBar 还原纯窗口控制"
```

---

### Task 3: 小收尾（连接点颜色 + AGENTS.md）

**Files:**
- Modify: `frontend/src/lib/StatusBar.svelte`
- Modify: `AGENTS.md`

- [ ] **Step 1: StatusBar `.dot` 颜色令牌化**

`frontend/src/lib/StatusBar.svelte` 第 ~35 行：
```css
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #666; transition: background 0.3s; }
```
改为 `background: var(--text-dim);`。

- [ ] **Step 2: AGENTS.md 更新**

在 Tauri GUI / 前端组件区域补充（沿用现有紧凑风格）：
- ProjectTabs：滚动标签区（`overflow-x: auto` + 滚轮水平滚动 action + 激活标签自动滚入视野 + 可滚动时两端 mask 渐隐 + hover 细滚动条）；右侧固定按钮区（☀/🌙 主题切换 + ⚙ 设置，细分隔线）；激活标签 hover 显示 ×（onRemoveRecent，复用确认逻辑）；props：`theme`/`onToggleTheme`/`onRemoveRecent`
- 主题切换按钮位于标签栏右侧（TitleBar 为纯窗口控制）

- [ ] **Step 3: 验证**

Run: `cd frontend; npm run check` → 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/StatusBar.svelte AGENTS.md
git commit -m "docs: 标签栏滚动与主题按钮位置说明"
```

---

## Self-Review（已执行）

- **用户需求覆盖**：溢出覆盖修复 → Task 1（结构拆分 + 滚动）；"滚轮滑动到后面仓库" → wheelScroll action + 滚动条 + 自动定位；主题按钮移到设置旁 → Task 2；美化 → Task 1（渐隐/分隔线/关闭 ×）+ Task 3（连接点颜色）。
- **占位符扫描**：无 TBD/TODO；所有代码块完整。
- **类型一致性**：`theme`/`onToggleTheme`/`onRemoveRecent` 在 Task 1 定义 props、Task 2 App 传入一致；`handleRemoveRecent` 为 App.svelte 现有函数（签名 `(recentPath: string) => void`）；Svelte action `wheelScroll` 返回值含 `destroy`（Svelte 5 action 协议）。
- **注意点**：`tab-close` 只在激活标签显示（避免误触）；关闭按钮 `stopPropagation` 防触发选中；`scrollIntoView` 在 `$effect` 中读取 activePath 触发。
