# 日夜双模式设计（人形日 / 白泽夜）

日期：2026-08-05
状态：已获用户批准（"可以执行"）

## 目标

基于上白泽慧音双形态为桌面端与网页端（web-timeline）提供日夜双主题：
- **日间 = 人形**：深蓝 + 白 + 红 + 冰蓝白发
- **夜间 = 白泽**：墨绿 + 白 + 红 + 绿白白发，绿黑底色
- 切换为**纯手动两态**（不跟随系统），两端各自持久化

## 色板映射

| 令牌 | 日（人形） | 夜（白泽） |
|---|---|---|
| `--accent` 正红（固定） | `#D94747` / hover `#C63B3B` | `#E05555` / `#C94545`（暗底提亮） |
| `--accent-soft` | `#FBEDED` | `#3A2424` |
| `--blue`（深蓝→墨绿） | `#2E4A6B` | `#4A7A5C` |
| `--blue-soft` | `#EBF1F7` | `#1E2E26` |
| `--ze` | `#3E6B52` / hover `#335A44` | `#57A07F` / `#4A8F70` |
| `--ze-soft` | `#EAF2EC` | `#1E2E26` |
| `--ice`（冰蓝→绿白） | `#7FA8CE` | `#B8E6C8` |
| `--ice-soft` | `#EFF5FB` | `#1F2D26` |
| `--bg` | `#F4F6F8` | `#141D18` 绿黑 |
| `--surface` | `#FFFFFF` | `#1B2620` |
| `--surface2` | `#EDF0F4` | `#22302A` |
| `--surface3` | `#DCE1E8` | `#2C3D35` |
| `--border` | `#E3E7ED` | `#2E3F37` |
| `--text` | `#1E2833` | `#E8F2EC` |
| `--text-dim` | `#6B7684` | `#93A8A0` |
| `--success` | `#3E6B52` | `#57A07F` |
| `--warning` | `#B0803A` | `#D4A04D` |
| `--error` | `#C94343` | `#E05C5C` |
| `--card-active`（新） | `#FFFDFD` | `#1F2C25` |
| `--warning-soft`（新） | `rgba(176,128,58,0.12)` | `rgba(212,160,77,0.14)` |
| 阴影 | 现值 | `rgba(0,0,0,0.40/0.45/0.55)` |

红/白为双形态固定色；夜间整体替换人形色系为白泽色系。

## 切换机制（纯手动两态）

- **桌面端**：标题栏加 ☀/🌙 按钮（`-webkit-app-region: no-drag`）；状态 `localStorage('keinestamp_theme')` 默认 `day`；`App.svelte` `$state` + `$effect` 写 `<html data-theme="day|night">`；`frontend/index.html` 头部加内联预读脚本（防夜间启动白闪）
- **网页端**：左侧栏 footer 加 ☀/🌙 按钮；`localStorage('keinestamp_web_theme')` 默认 `day`；同一 `data-theme` 机制；头部内联预读
- 两端各自持久化、互不同步（不同源）

## 标签色自适应（color-mix 取代 tagSoft）

内联只设 `--tag-c`，徽章配色用 CSS `color-mix`（WebView2/Chromium ≥111 支持），**同一公式双主题自动适配**：

```css
.tag-badge {
  color: color-mix(in srgb, var(--tag-c) 45%, white);
  background: color-mix(in srgb, var(--tag-c) 14%, transparent);
}
```

- 删除 `tags.ts` 的 `tagSoft`；EntryList/DetailPanel 徽章改为 `style="--tag-c:{tagColor(tag)}"`
- 网页端徽章同步改造（内联 `background:` 改 `--tag-c`）

## 网页端（html_gen.rs 模板）改造

- 现有 Catppuccin 深色色板（~35 处硬编码）全部变量化：`:root`（日，镜像桌面令牌）+ `[data-theme="night"]`（白泽）
- diff/状态色复用令牌：add→`--ze`/`--ze-soft`、del→`--error`/`--accent-soft`、mod→`--blue`/`--blue-soft`
- 日历热度色（h1-h4 绿阶）保持硬编码（语义色，双主题通用）
- **3D 查看器保持深色不变**（`.model-box`/`.model-toolbar`/`.model-overlay`/`.mbtn` 等）—— 3D 视口深色在双主题下都合适
- `.entry.tagged` 回退色 `#89b4fa` → `var(--accent)`
- 标签徽章/分组点改 `--tag-c` + color-mix

## 明确不做

- 不跟随系统自动切换；不做主题过渡动画；不改 Rust 业务逻辑；3D 查看器不随主题变色

## 测试

- `npm run check`（0 errors）+ `cargo test`（67+7 基线 + Task 3 新增集成断言：生成 HTML 含 `[data-theme="night"]` 块与切换脚本/存储键）
- 手动验证清单：桌面双主题切换 + 刷新持久化 + 夜间标签徽章可读；网页端部署后双主题切换
