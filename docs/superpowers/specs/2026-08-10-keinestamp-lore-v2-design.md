# Keinestamp v2（Lore 原生重设计）设计规格

**日期**: 2026-08-10
**状态**: 已批准（用户确认方向与全部决策点）

## 背景

Keinestamp v1 是"Blender 项目 Git 进度看板"（Rust 2024 + Tauri v2 + Svelte 5）。问题：

- 每个 .blend 工程 30-40GB，git+LFS 每版本全量存储，空间线性增长
- 回退机制依赖 git 底层对象操作（stash/refs/手工三向合并），复杂且脆弱
- 程序深度绑定 git 语义（skip-worktree、.gitattributes、LFS 配置、独立 web-timeline 仓库）

目标：基于 **Epic Games Lore**（0.8.x，分块去重版本控制系统）重新设计后端与 UI。

## 决策记录（用户已确认）

| # | 决策 | 内容 |
|---|---|---|
| D1 | 整体替换 | 后端 git 全部替换为 Lore，非双后端并存 |
| D2 | 基于 Lore 模型重设计 | 不翻译 git 语义，按 Lore 能力设计 UI/功能 |
| D3 | 回退简化 | `revision restore`（回到此版本）/ `revert`（撤销回退）/ `branch reset`（顶替）；删除 stash/refs/三向合并状态机 |
| D4 | 元数据权威 | tag/message/milestone 存 `revision metadata`；timeline.json 仅作展示缓存（web 部署用） |
| D5 | web 部署并入 | web 产物（html + GLB）进 `web/` 子目录（主 Lore 仓库内），程序自动提交，metadata `kind=web` 标记；时间线/版本计数按 kind 过滤；删除独立 web-timeline git 仓库 |
| D6 | 隐藏机制 | GUI 本地隐藏列表（app-config 层）替代 gitignore + skip-worktree；`web/` 预设隐藏 |
| D7 | 服务器 | serve（局域网 HTTP 服务器）拉起 `loreserver` 子进程；GUI 备份按钮 = `lore push` 到局域网服务器 |
| D8 | 不迁移历史 | 程序未投产，旧 git 工程不迁移，直接进入 Lore 原生模式 |
| D9 | 接入方式 | Lore CLI 子进程封装（同 v1 的 git_helpers 模式），锁版本 v0.8.6 |
| D10 | 验证门前置 | 未验证项（untracked 检测/ignore/restore 语义/loreserver/去重率）先验证，不达标止损 |

## 目标架构

```
Keinestamp v2
├─ GUI (Tauri/Rust/Svelte5) ── CLI 子进程封装（lore_backend.rs）
│    └─ Lore 主仓库（本地模式）
│         ├─ 用户文件（blend/模型/贴图）→ 用户提交 kind=user
│         └─ web/ 子目录（隐藏）→ 程序自动提交 kind=web
└─ serve（局域网多租户 HTTP 服务器）
     ├─ 现有 web 部署端点（不变）
     └─ loreserver 子进程（内部端口）← GUI `lore push` 备份
```

## 数据流

- **record**: 用户勾选文件（status 输出 - 隐藏列表）→ `lore file stage` + `lore commit`（metadata: kind=user, message, tag）→ timeline.json 重建（展示缓存）
- **rollback**: 时间线选节点 → `lore revision restore` 工作区还原；恢复 → `lore revision revert`/cherry-pick；回退后记录顶替 → `lore branch reset`
- **web 部署**: html_gen 生成页面 → `web/` 目录 → 自动 commit（kind=web）→ zip 打包 web/ → 上传服务器（现有 push.rs 流程）；版本号 = kind=web 的 revision 计数
- **备份**: 按钮 → `lore push` → serve 内 loreserver（局域网）

## 非目标（YAGNI）

- 历史迁移（D8）
- Lore 文件锁 UI（单人场景）
- 稀疏水合/按需下载 UI（单人本地全量）
- link/layer 多仓库组合（Lore 2026 roadmap 未完成）
- NAS 上独立 Docker lore-server（serve 集成已覆盖）

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| Lore 0.x API 变化（每 2 周一版） | 锁版本 v0.8.6；所有命令调用集中在 lore_backend.rs |
| 未验证能力缺失（untracked/ignore/restore） | 阶段 0 验证门，缺失则启用 GUI 层替代方案 |
| 去重率不达预期 | 验证门止损线：.blend 连存 10 次去重率 ≥50% |
| loreserver 认证配置（0.x 仍在变） | 局域网内部 token 简化方案，隔离在 serve.rs 集成层 |

## 成功标准

1. 新工程全流程可用：init → record（多版本）→ 时间线 → 回退/恢复/顶替 → 导出 → diff → web 部署 → 备份 push
2. 去重生效：10 次 .blend 存档仓库增量远小于全量
3. 回退 UX：无隐藏状态机（恢复按钮与冲突对话框逻辑显著简化）
4. 现有 100+ 单测 + 8 集成测试适配通过（lore 语义版本）
