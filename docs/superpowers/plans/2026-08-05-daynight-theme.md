# 日夜双模式实现计划（人形日 / 白泽夜）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为桌面端与网页端（web-timeline）添加日夜双主题：日=人形（白底+深蓝+冰蓝+正红），夜=白泽（绿黑底+墨绿+绿白+正红）。纯手动两态切换，localStorage 持久化，两端同构 `data-theme` 机制。

**Architecture:** 桌面端：`app.scss` 增加 `:root`（日）+ `[data-theme="night"]`（夜）双色板令牌块，`App.svelte` 持有 `theme` 状态经 `$effect` 写入 `<html data-theme>`，TitleBar 加 ☀/🌙 切换按钮，`index.html` 头部内联预读脚本。网页端：`html_gen.rs` 模板内 ~35 处硬编码色值变量化，同样 `:root`+`[data-theme="night"]` 双块，左侧栏 footer 加切换按钮 + 存储 JS。标签徽章统一改为内联 `--tag-c` + CSS `color-mix`（双主题自动适配），删除 `tagSoft`。3D 查看器保持深色不变。

**Tech Stack:** SCSS 变量、Svelte 5（`$state`/`$effect`/`$props`）、`data-theme` 属性、localStorage、CSS `color-mix`、Rust `html_gen.rs` 模板字符串。

## Global Constraints

- 纯手动两态：不读 `prefers-color-scheme`、不做自动跟随
- 桌面 localStorage 键：`keinestamp_theme`（'day'|'night'，默认 'day'）；网页端：`keinestamp_web_theme`（同默认）
- 夜间色板值以本计划 Task 1 的 `[data-theme="night"]` 块为准；红/白固定色不变
- 标签徽章统一 `--tag-c` + `color-mix`（WebView2/Chromium ≥111 支持，目标浏览器均满足）；删除 `tagSoft`（桌面 tags.ts + 两处 import）
- 网页端 3D 查看器颜色（`.model-box`/`.model-toolbar`/`.model-overlay`/`.mbtn`/`.model-expand-btn`/`.model-overlay-close`）保持深色硬编码不变
- 日历热度色（`.cal-cday.h1-h4`、`.cal-legend .s.s1-s4`）保持硬编码不变
- 验证命令：`cd frontend; npm run check`（0 errors）+ `cargo test`（基线 67 unit + 7 integration）
- 根目录 `package-lock.json` 永不提交（已删除，勿重新创建）
- 提交消息格式：`feat(theme): ...`
- 除 html_gen.rs 模板 JS 外，不改任何 Rust 业务逻辑

---

### Task 1: 桌面端双主题基建（app.scss + App.svelte + TitleBar + index.html）

**Files:**
- Modify: `frontend/src/app.scss`（`:root` 增令牌 + 追加 `[data-theme="night"]` 块）
- Modify: `frontend/src/App.svelte`（theme 状态）
- Modify: `frontend/src/lib/TitleBar.svelte`（切换按钮 + props）
- Modify: `frontend/index.html`（内联预读脚本）

**Interfaces:**
- Consumes: 现有全部 `var(--x)` 令牌体系（Task 无前置依赖）
- Produces:
  - app.scss 新令牌：`--card-active: #fffdfd`、`--warning-soft: rgba(176, 128, 58, 0.12)`（`:root`）；`[data-theme="night"]` 全量覆盖块（值见下方代码）
  - `App.svelte` 状态：`theme: 'day' | 'night'`（localStorage 初始化）+ `$effect` 写 `document.documentElement.dataset.theme` 与 localStorage
  - `TitleBar.svelte` 新 props：`theme: string`（默认 'day'）、`onToggleTheme: () => void`；模板在 `.tb-controls` 前加 ☀/🌙 按钮（含 `-webkit-app-region: no-drag` 继承自 `.tb-btn`）

- [ ] **Step 1: app.scss `:root` 增两个令牌（在 `--accent2` 附近）**

```scss
  --card-active: #fffdfd;
  --warning-soft: rgba(176, 128, 58, 0.12);
```
并把 `.entry-card.active` 规则中的 `background: #fffdfd;` 改为 `background: var(--card-active);`。

- [ ] **Step 2: app.scss 文件末尾追加夜间块**

```scss
/* ---- 夜间模式（白泽形态） ---- */
[data-theme="night"] {
  --accent: #e05555;
  --accent-hover: #c94545;
  --accent-soft: #3a2424;
  --blue: #4a7a5c;
  --blue-soft: #1e2e26;
  --ze: #57a07f;
  --ze-hover: #4a8f70;
  --ze-soft: #1e2e26;
  --ice: #b8e6c8;
  --ice-soft: #1f2d26;
  --bg: #141d18;
  --surface: #1b2620;
  --surface2: #22302a;
  --surface3: #2c3d35;
  --border: #2e3f37;
  --text: #e8f2ec;
  --text-dim: #93a8a0;
  --success: #57a07f;
  --warning: #d4a04d;
  --error: #e05c5c;
  --card-active: #1f2c25;
  --warning-soft: rgba(212, 160, 77, 0.14);
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.4);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.45);
  --shadow-lg: 0 16px 48px rgba(0, 0, 0, 0.55);
}
```

- [ ] **Step 3: App.svelte 脚本区（`let deploying = $state(false)` 之后）**

```ts
  let theme = $state<'day' | 'night'>(localStorage.getItem('keinestamp_theme') === 'night' ? 'night' : 'day')

  $effect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem('keinestamp_theme', theme)
  })
```

模板中 `<TitleBar />` 改为：

```svelte
  <TitleBar theme={theme} onToggleTheme={() => theme = theme === 'day' ? 'night' : 'day'} />
```

- [ ] **Step 4: TitleBar.svelte**

props 块（第 1-9 行脚本）追加：

```ts
  let {
    theme = 'day',
    onToggleTheme = () => {},
  }: {
    theme?: string
    onToggleTheme?: () => void
  } = $props()
```

模板在 `.tb-controls` div 之前（`.tb-spacer` 之后）插入：

```svelte
  <button class="tb-btn" onclick={onToggleTheme} title={theme === 'night' ? '切换到日间模式' : '切换到夜间模式'}>
    {#if theme === 'night'}
      <svg viewBox="0 0 12 12" width="11" height="11" fill="currentColor"><path d="M6 1a5 5 0 100 10A5 5 0 016 1z"/></svg>
    {:else}
      <svg viewBox="0 0 12 12" width="11" height="11" fill="currentColor"><path d="M6 3a3 3 0 100 6 3 3 0 000-6zm0-2a.5.5 0 01.5.5v1a.5.5 0 01-1 0v-1A.5.5 0 016 1zm0 7a.5.5 0 01.5.5v1a.5.5 0 01-1 0v-1A.5.5 0 016 8zm4-2a.5.5 0 010 1H9a.5.5 0 010-1h1zM2 6a.5.5 0 01.5-.5H4a.5.5 0 010 1H2.5A.5.5 0 012 6zm3.646-3.354a.5.5 0 01.708 0l.707.707a.5.5 0 01-.707.708l-.708-.707a.5.5 0 010-.708zm3.354 3.354a.5.5 0 01.708 0l.707.707a.5.5 0 01-.708.708l-.707-.708a.5.5 0 010-.707z"/></svg>
    {/if}
  </button>
```
（`.tb-btn` 已含 `-webkit-app-region: no-drag`，无需新样式。）

- [ ] **Step 5: frontend/index.html `<head>` 内 `<title>` 前加预读脚本**

```html
    <script>
      document.documentElement.dataset.theme = localStorage.getItem('keinestamp_theme') === 'night' ? 'night' : 'day'
    </script>
```

- [ ] **Step 6: 验证**

Run: `cd frontend; npm run check`
Expected: 0 errors。
目检（若可行）：启动后点 ☀/🌙 全界面日夜切换；夜间下卡片/弹窗/Toast/侧栏配色为绿黑底白泽色；刷新后保持。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/app.scss frontend/src/App.svelte frontend/src/lib/TitleBar.svelte frontend/index.html
git commit -m "feat(theme): 桌面端日夜双主题基建（白泽夜间色板 + 手动切换）"
```

---

### Task 2: 桌面标签色 color-mix 改造

**Files:**
- Modify: `frontend/src/lib/tags.ts`（删除 `tagSoft`）
- Modify: `frontend/src/lib/EntryList.svelte`（import、徽章内联样式、`.st-mod` 用 `--warning-soft`、`.tag-badge` 样式）
- Modify: `frontend/src/lib/DetailPanel.svelte`（import、徽章内联样式、`.tag-badge` 样式）

**Interfaces:**
- Consumes: Task 1 的 `--warning-soft` 令牌
- Produces: `.tag-badge` 仅内联 `--tag-c`；`.tag-badge` CSS 用 `color-mix` 双公式；`tagSoft` 全库删除

- [ ] **Step 1: `tags.ts` 删除 `tagSoft` 函数（保留 TAG_COLORS/FALLBACK_COLORS/tagColor）**

- [ ] **Step 2: `EntryList.svelte`**

- import 行 `import { tagColor, tagSoft } from './tags'` 改为 `import { tagColor } from './tags'`
- 徽章（模板中 `style="color:{tagColor(entryTag)};background:{tagSoft(entryTag)}"`）改为：

```svelte
            <span class="tag-badge" style="--tag-c:{tagColor(entryTag)}">{entryTag}</span>
```

- `<style>` 中 `.tag-badge` 规则改为：

```css
  .tag-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    margin-left: 6px;
    vertical-align: middle;
    color: color-mix(in srgb, var(--tag-c) 45%, white);
    background: color-mix(in srgb, var(--tag-c) 14%, transparent);
  }
```

- `<style>` 中 `.st-mod` 的 `background: rgba(176, 128, 58, 0.12);` 改为 `background: var(--warning-soft);`

- [ ] **Step 3: `DetailPanel.svelte`**

- import 行改为 `import { tagColor } from './tags'`（若原样含 tagSoft 则去掉）
- 徽章（`style="color:{tagColor(entry.tag)};background:{tagSoft(entry.tag)}"`）改为：

```svelte
          <span class="tag-badge" style="--tag-c:{tagColor(entry.tag)}">{entry.tag}</span>
```

- `.tag-badge` 规则改为与 EntryList 相同的 color-mix 双公式（font-size 11px / padding 2px 10px 等其余属性保留现值）

- [ ] **Step 4: 验证**

Run: `cd frontend; npm run check`
Expected: 0 errors（若 svelte-check 报 `--tag-c` 相关类型/样式告警为噪音，确认无 error 即可）。
Run（确认无残留）: 在 `frontend/src/` 中搜索 `tagSoft`，应无任何命中。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/tags.ts frontend/src/lib/EntryList.svelte frontend/src/lib/DetailPanel.svelte
git commit -m "feat(theme): 标签徽章改 color-mix 双主题自适应"
```

---

### Task 3: 网页端双主题（html_gen.rs 模板改造）

**Files:**
- Modify: `src/html_gen.rs`
- Test: `tests/cli_tests.rs`（新增 1 个集成测试）

**Interfaces:**
- Consumes: 双色板语义（与桌面令牌同名同值，见下方代码）
- Produces: 生成 index.html 含 `:root`（日）+ `[data-theme="night"]`（夜）CSS 变量、预读脚本、左侧栏 ☀/🌙 按钮与切换 JS、`keinestamp_web_theme` 存储键、`--tag-c` + color-mix 标签徽章

- [ ] **Step 1: `<head>` 加预读脚本 + 替换 `<style>` 内的颜色为变量**

`<meta charset="UTF-8">` 之后、`<title>` 之前插入：

```html
<script>document.documentElement.dataset.theme = localStorage.getItem('keinestamp_web_theme') === 'night' ? 'night' : 'day';</script>
```

在 `<style>` 开头（`* { margin: 0; ... }` 之前）插入双色板变量块：

```css
:root {
  --bg: #f4f6f8; --surface: #ffffff; --surface2: #edf0f4; --surface3: #dce1e8;
  --border: #e3e7ed; --text: #1e2833; --text-dim: #6b7684; --text-2: #506070;
  --accent: #d94747; --accent-soft: #fbeded;
  --blue: #2e4a6b; --blue-soft: #ebf1f7;
  --ze: #3e6b52; --ze-soft: #eaf2ec;
  --warning: #b0803a; --error: #c94343; --warning-soft: rgba(176,128,58,0.12);
  --card-active: #fffdfd;
  --shadow-sm: 0 1px 2px rgba(30,40,51,0.06);
}
[data-theme="night"] {
  --bg: #141d18; --surface: #1b2620; --surface2: #22302a; --surface3: #2c3d35;
  --border: #2e3f37; --text: #e8f2ec; --text-dim: #93a8a0; --text-2: #b5c4bc;
  --accent: #e05555; --accent-soft: #3a2424;
  --blue: #4a7a5c; --blue-soft: #1e2e26;
  --ze: #57a07f; --ze-soft: #1e2e26;
  --warning: #d4a04d; --error: #e05c5c; --warning-soft: rgba(212,160,77,0.14);
  --card-active: #1f2c25;
  --shadow-sm: 0 1px 2px rgba(0,0,0,0.4);
}
```

逐条替换现有硬编码颜色（保留其余属性不变）：
- `body`：`background: #1e1e2e` → `var(--bg)`；`color: #cdd6f4` → `var(--text)`
- `.left-col`：`background: #181825` → `var(--surface)`；`border-right: 1px solid #313244` → `1px solid var(--border)`
- `h1`：`color: #f5f5f7` → `var(--text)`
- `.subtitle`：`#6c7086` → `var(--text-dim)`
- `.entry`：`background: #181825` → `var(--surface)`；`border: 1px solid #313244` → `1px solid var(--border)`；`border-left: 3px solid #313244` → `3px solid var(--border)`
- `.entry.tagged`：`var(--tag-color, #89b4fa)` → `var(--tag-color, var(--accent))`
- `.entry-id`/`.entry-date`：`#6c7086` → `var(--text-dim)`
- `.entry-message`：`#f5f5f7` → `var(--text)`
- `.tag-badge`：`color: #1e1e2e` 删除，改 `color: color-mix(in srgb, var(--tag-c) 45%, white); background: color-mix(in srgb, var(--tag-c) 14%, transparent);`
- `.empty`：`#6c7086` → `var(--text-dim)`
- `.tag-group-label`：`#cdd6f4` → `var(--text)`
- `.tag-group-line`：`#313244` → `var(--border)`
- `.sec-hdr`：`color: #6c7086` → `var(--text-dim)`；`border-top: 1px solid #313244` → `1px solid var(--border)`
- `.sec-hdr:hover`：`#cdd6f4` → `var(--text)`
- `.fs-path`：`#cdd6f4` → `var(--text)`
- `.st-add`：`#4caf88` → `var(--ze)`
- `.st-del`：`#e06c75` → `var(--error)`
- `.st-mod`：`#e5c07b` → `var(--warning)`
- `.diff-path`：`color: #89b4fa` → `var(--blue)`；`background: rgba(0,0,0,0.25)` → `var(--blue-soft)`
- `.diff-empty`/`.diff-loading`：`#6c7086` → `var(--text-dim)`
- `.dl-num`：`#6c7086` → `var(--text-dim)`
- `.dl.add`：`background: rgba(76,175,136,0.12); color: #4caf88` → `background: var(--ze-soft); color: var(--ze)`
- `.dl.add .dl-num`：`#4caf88` → `var(--ze)`
- `.dl.del`：`rgba(224,108,117,0.12)` / `#e06c75` → `var(--accent-soft)` / `var(--error)`
- `.dl.del .dl-num`：`#e06c75` → `var(--error)`
- `.dl.mod`：`rgba(100,140,255,0.12)` / `#648cff` → `var(--blue-soft)` / `var(--blue)`
- `.dl.mod .dl-num`：`#648cff` → `var(--blue)`
- `.cal-title`：`#cdd6f4` → `var(--text)`
- `.cal-mlabel`：`#a6adc8` → `var(--text-2)`
- `.cal-cday`：`background: #313244` → `var(--surface3)`；`outline: 2px solid #89b4fa` → `2px solid var(--accent)`
- `.cal-legend`：`color: #585b70` → `var(--text-dim)`
- `.cal-legend .s`：`background: #313244` → `var(--surface3)`
- `.left-footer-btn`：`background: rgba(69,71,90,0.3)` → `var(--surface2)`；`border: 1px solid #45475a` → `1px solid var(--border)`；`color: #cdd6f4` → `var(--text)`
- `.cal-cday.h1-h4` 与 `.cal-legend .s.s1-s4` 热度绿阶：**保持硬编码不变**
- 3D 查看器类（`.model-placeholder`/`.model-box`/`.model-toolbar`/`.mbtn`/`.model-expand-btn`/`.model-overlay`/`.model-overlay-bar`/`.model-overlay-close`）：**保持深色硬编码不变**

- [ ] **Step 2: 左侧栏 footer 加切换按钮**

模板中：

```html
    <div class="left-footer">
      <span style="font-size:10px;color:#6c7086;text-align:center">Keinestamp</span>
    </div>
```

改为：

```html
    <div class="left-footer">
      <button id="theme-toggle" class="left-footer-btn" style="margin-top:6px">☀ 切换日/夜</button>
      <span style="font-size:10px;color:var(--text-dim);text-align:center;margin-top:8px">Keinestamp</span>
    </div>
```
（若 `.left-footer` 现有布局是纵向 flex，保持；若未定义样式则无需新 CSS，按钮用现有 `.left-footer-btn` 类样式。）

- [ ] **Step 3: 模板 JS 加切换逻辑（`<script>` 开头，`var data = ...` 之前）**

```js
document.getElementById('theme-toggle').addEventListener('click', function() {
  var next = document.documentElement.dataset.theme === 'night' ? 'day' : 'night';
  document.documentElement.dataset.theme = next;
  localStorage.setItem('keinestamp_web_theme', next);
});
```

- [ ] **Step 4: 标签徽章 JS（line ~304）**

```js
      if (tag) h += '<span class="tag-badge" style="--tag-c:' + tagColor(tag) + '">' + esc(tag) + '</span>';
```
（分组点 line ~280 的 `var c = tagColor(tag)` 内联色保留不变 —— 色点在双主题下均可见。）

- [ ] **Step 5: 集成测试（tests/cli_tests.rs 追加）**

参考现有测试风格（`$env:TEMP/keinestamp_int_*`，若现有测试有可复用的 init 流程则复用；否则最小构造：临时项目目录 + `MetaConfig` + 空 timeline + `generate_index_html`）：

```rust
#[test]
fn html_gen_contains_daynight_theme() {
    // 构造临时项目：meta（web_root 指向临时 web-timeline 目录）、空 Timeline
    // 调用 keinestamp::html_gen::generate_index_html(...)
    // 读取输出 index.html，断言：
    //   - 包含 `[data-theme="night"]`
    //   - 包含 `keinestamp_web_theme`
    //   - 包含 `theme-toggle`
    //   - 包含 `color-mix`
}
```
（Timeline 构造参考 `src/timeline.rs` 的结构体定义；若空 timeline 无法生成 html，造一条最小 entry。）

- [ ] **Step 6: 验证**

Run: `cargo test`
Expected: 新增测试通过；67+7 基线无回归。

- [ ] **Step 7: Commit**

```bash
git add src/html_gen.rs tests/cli_tests.rs
git commit -m "feat(theme): 网页端日夜双主题（模板变量化 + 切换按钮）"
```

---

### Task 4: 收尾验证与文档

**Files:**
- Modify: `AGENTS.md`
- 验证: 全量测试 + 手动清单

- [ ] **Step 1: 全量验证**

Run: `cd frontend; npm run check` → 0 errors
Run: `cargo test` → 全过

- [ ] **Step 2: AGENTS.md 更新**

在 Tauri GUI 段落补充（沿用现有风格）：
- 日夜双主题：日=人形（白底+深蓝+冰蓝+正红）、夜=白泽（绿黑底+墨绿+绿白+正红）；`[data-theme="night"]` 覆盖块在 app.scss；手动 ☀/🌙 切换，localStorage 键 `keinestamp_theme`（桌面）/`keinestamp_web_theme`（网页）；index.html 头部预读防白闪
- 标签徽章：内联 `--tag-c` + CSS `color-mix` 双主题自适应（无 tagSoft）
- 网页端：html_gen.rs 模板 `:root`/`[data-theme="night"]` 双色板；3D 查看器与日历热度色保持深色/硬编码

- [ ] **Step 3: 手动验证清单（留给用户）**

1. 桌面：☀/🌙 切换，日/夜配色正确，刷新后保持，夜间标签徽章可读
2. 部署网页：打开页面默认日间，切夜间，刷新保持；两主题下 diff/状态/标签配色正常
3. 老项目兼容：旧部署网页仍可看（无 JS 异常）

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md
git commit -m "docs: 日夜双主题说明"
```

---

## Self-Review（已执行）

- **Spec 覆盖**：桌面基建（色板/切换/预读）→ Task 1；桌面标签 color-mix → Task 2；网页端（变量化/按钮/JS/测试）→ Task 3；文档+验证 → Task 4。不做的（不跟随系统、无动画、3D 查看器不变、日历热度色不变）→ Global Constraints 与 Task 3 清单。
- **占位符扫描**：无 TBD/TODO；Task 3 的逐条颜色替换给出明确映射表。
- **类型一致性**：`keinestamp_theme`/`keinestamp_web_theme` 键全程一致；`--tag-c` 内联变量在桌面两组件与网页端一致；`tagSoft` 在 Task 2 删除后全库无引用（Task 3 网页端模板原本就没有 tagSoft，只有内联 background，已一并改）。
- **注意**：Task 1 改 `TitleBar.svelte` props —— App.svelte 的调用点同步更新，svelte-check 会验证。
