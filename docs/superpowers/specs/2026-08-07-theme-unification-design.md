# 2026-08-07 三端配色统一 + 时间线页 B/C 两版美化

## 背景

- 首页（landing page）为"灰阶 + 单红 accent + 三灰调 deco"编辑风，用户认可其设计语言
- 但纯红白黑丢失了项目设定色：Day 人形（淡蓝/冰蓝）、Night 白泽（淡绿/青绿）
- 目标：以首页设计语言为骨架重构三端（GUI / 时间线页 / 首页）配色，保留淡蓝/绿双形态；时间线页按首页风格做 B（中度）/C（重度）两版，先出预览文件，确认后并入

## 配色令牌（三端统一，提案 1）

`--blue / --ze / --ice` 三变量合并为单一 `--tint / --tint-soft`（day=冰蓝、night=青绿，自动跟随形态）。

### Day（人形）

| 变量 | 值 |
|---|---|
| `--bg` / `--surface` | `#f5f6f8` / `#ffffff` |
| `--surface2` / `--surface3` | `#eef1f4` / `#dfe4ea` |
| `--text` / `--text-dim` / `--text-2` | `#1b222b` / `#69727c` / `#506070` |
| `--line` / `--border` | `#d9dee5` |
| `--accent` / `--accent-hover` / `--accent-soft` | `#d94747` / `#c63b3b` / `#fbeded` |
| `--tint` / `--tint-soft` | `#7fa8ce` / `#eff5fb` |
| `--success` / `--warning` / `--error` | `#5d8a75` / `#b0803a` / `#c94343` |

### Night（白泽）

| 变量 | 值 |
|---|---|
| `--bg` / `--surface` | `#151a18` / `#1b211f` |
| `--surface2` / `--surface3` | `#242c29` / `#2f3a35` |
| `--text` / `--text-dim` / `--text-2` | `#e9efeb` / `#9aa5a0` / `#c3cdc7` |
| `--line` / `--border` | `#2a3237` |
| `--accent` / `--accent-hover` / `--accent-soft` | `#e05a5a` / `#cf4a4a` / `#4a2b2d` |
| `--tint` / `--tint-soft` | `#6fc49a` / `#25362e` |
| `--success` / `--warning` / `--error` | `#63b58b` / `#ddb25e` / `#e96868` |

## 时间线页 B 版（`?layout=classic`，并入后默认）

- 换配色令牌 + 三色合并（`--blue/--ze/--ice` → `--tint`）
- 标题区：h1 上方加 eyebrow（红线 + `Blender · Git · Timeline`），副标题改首页式代码块（`$ record 3a1f9c2 ✓ 已记录`）
- 卡片：圆角 16px、阴影对齐首页（`--shadow-sm` / `--shadow-hover`）、hover 上浮保留
- 轻装饰：背景左上/右下 2 个 tint 光晕（低透明 blur，`color-mix` tint 7%~10%）

## 时间线页 C 版（`?layout=hero`）

- 顶部 hero 头部（首页风格）：topbar 品牌条（logo + 项目名）+ hero 左文右画（eyebrow / 大标题 / 代码块副标题 / 右侧月相盘 + 圆环装饰）
- 背景装饰层：顶部波形 + 双光晕 + 代码纹理（4 处绝对定位低透明）——复用首页装饰 CSS
- 左栏保留，视觉首页化（surface 卡片质感 + 红 accent 按钮）
- 卡片首页化：圆角 16px、阴影、hover 上浮 + 红边

## GUI（app.scss）

仅换配色令牌（`:root` + `[data-theme="night"]` 两块变量），组件内 `--blue/--ze/--ice` 引用点改为 `--tint`（DetailPanel / StatusPanel / EntryList / Toast 等，grep 定位）；布局零改动。

## 首页（serve.rs landing page）

仅微调：`--deco-a/b` 改为形态色灰调（day 蓝灰 `#9ab3cd`、night 绿灰 `#7fa08d`），其余不动。

## 交付流程

1. 独立预览 HTML（B/C 各一，Temp 目录）：样例时间线数据（条目/标签/里程碑/文件/diff/日历）、双主题切换按钮、3D 区占位、`?theme=` 支持
2. 用户确认版式 → 并入 html_gen.rs（双模板 + layout 参数）、app.scss、serve.rs deco 微调
3. `cargo test` + `npm run check` + 端到端 → 提交推送

## 验收标准

- 三端 day/night 视觉统一（灰阶 + 红 + 淡蓝/淡绿形态）
- 时间线页 B/C 两版可经 `?layout=` 切换，功能不受影响
- GUI 布局零改动，仅颜色变化
- 全部现有测试通过
