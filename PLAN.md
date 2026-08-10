# Keinestamp v2 计划

**状态**: 已落定（2026-08-11，lore 分支）。实施计划与设计规格见：
- `docs/superpowers/plans/2026-08-10-keinestamp-lore-v2.md`（Task 0-11）
- `docs/superpowers/specs/2026-08-10-keinestamp-lore-v2-design.md`（决策 D1-D10）
- `docs/lore-commands.md`（Lore v0.8.6 实测速查表）

## 完成情况

| Task | 内容 | 提交 |
|---|---|---|
| 0 | 克隆 v1 → lore 分支（clone 替代 robocopy） | `68229f2` |
| 1 | 验证门：Lore 能力实测 + 速查表（去重率 99.9%） | `57b9385` |
| 2 | lore_backend.rs 基础封装（repo/status/commit/history） | `b3df3f5` |
| 3 | hidden 隐藏列表 + lore_project 记录流程 | `a46d81b` |
| 4 | 回退/撤销回退/顶替（branch reset + dirty 备份） | `d63e910` |
| 5 | 导出/文件 diff + timeline 从 history 重建 | `5b0d3af` |
| 6 | web 产物并入主仓库（kind=web），删 git_web | `60c609f` |
| 7 | CLI 命令 lore 化（删 git_project/git_helpers） | `048710d` |
| 8 | Tauri 命令 lore 化 | `2109794` |
| 9 | 前端 UI 适配（statusParse/隐藏管理/删冲突 UI） | `6aba7b5` |
| 10 | serve 拉起 loreserver + backup 命令 | `358bd96` |
| 11 | 收尾：清理 + 文档（本文档 + AGENTS.md/README） | 当前 |

## 关键结论（Task 1 实测）

- `lore revision restore` 在 0.8.6 不可用（Invalid head）→ 回退 = `branch reset`
- 撤销回退 = `branch reset <回退前 HEAD sig>`（timeline.json marker 记录 rollback_point + head_before）
- revert = 撤销某 revision 改动（冲突 resolve mine/theirs），非"撤销回退"
- Lore 无原生忽略规则 → 隐藏列表方案（app-config）
- 所有操作依赖 loreserver 在线（无纯本地模式）→ serve/GUI 拉起
- 空提交被拒（"Nothing staged for commit"）→ record 需有实际改动
- 去重率 99.9%（6MB × 10 次增长 81KB）
