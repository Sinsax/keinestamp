# Rollback 回退/恢复 新机制设计

日期：2026-08-09
状态：已批准

## 背景与问题

原「撤回/恢复」（revert/restore_revert）存在三个缺陷：

1. **全局单槽位丢失**：`last_revert` 存在 AppConfig（全局文件），任何项目的撤回都会覆盖上一个；集成测试 `test_init_then_revert` 会在真实 app-config.toml 上执行撤回，污染真实用户配置（实测发生）。
2. **语义不符合工程直觉**：原实现 `git reset --soft HEAD~1` 只动历史指针，文件永远是最新状态——用户期望"回退 = 文件真的回到所选记录的状态，且随时能回来"。
3. **恢复限制生硬**：原实现要求 HEAD == revert_hash，撤回后只要有任何新提交就永久无法恢复；被撤回的 commit 又可能被 gc。

## 目标

- 回退 = 文件真正回到所选记录的状态（工作区/索引），当前最新状态暂存
- 恢复 = 暂存内容合并带回，随时可用（回退状态下无新记录时）
- 零全局状态：回退状态完全存在项目自己的 `.git` 里，测试/其他项目/配置写入无法干扰
- 二进制安全：blender 工程冲突用文件级二选一解决，不做行级文本合并

## 核心语义

### 回退（右键时间线记录 → 回退）

```
git stash push -u -m "keinestamp-rollback <X> <HEAD>"   # 暂存最新状态（含未跟踪），message 编码回退点 hash + 回退时 HEAD hash
git restore --source=<X> --staged --worktree -- .        # 工作区 + 索引文件变为记录 X 的状态
```

- HEAD、git 历史、timeline.json 完全不动；只动文件与索引 + 产生一个 stash
- 暂存与回退两步保证零数据丢失：stash push 先拿走工作区差异，restore 再覆盖

### 恢复（时间线面板上侧按钮）

```
检测（get_rollback_state）：
  1. 第一条 stash 的 message 以 "keinestamp-rollback" 开头（回退点 X、回退时 HEAD）
  2. git rev-parse HEAD == message 中的回退时 HEAD（回退后没有新记录）
两者都满足 → 恢复按钮可用

执行：
  确认对话框（红字警告）+ 5s 倒计时后才可确认
  → git stash pop（三向合并，base = 回退前 HEAD）
  → 成功：结束回退状态，灰显取消，stash 已 drop
  → 冲突：进入冲突解决流程（见下）
```

### 冲突解决（GUI 文件级二选一）

`git stash pop` 对二进制/双改文件冲突时：

- 未冲突文件自动合并带回
- 冲突文件弹窗列出，每个文件二选一：
  - **当前版本**（回退期间改的，保留工作区现状）
  - **最新版本**（暂存内容，`git show stash@{0}:<path>` 读对象写回文件）
- 全部选完 →「完成恢复」→ 写回选中版本 → `git add` → `git stash drop` → 结束回退状态
- stash pop 失败时 stash 保留（不 drop），冲突标记/部分应用状态留在工作区
- 未跟踪文件同名冲突（already exists）：提示删除/重命名后重试

### 回退状态下的约束

- 回退状态下「记录」（commit）：CommitDialog 显示现有 restore-warn 警告（"撤回未恢复，继续记录将无法再恢复"，文案改"回退"），允许继续
- 记录成功后 HEAD 前进 → 恢复按钮失效；drop 匹配的 stash（"无法再恢复"语义成立，无残留）
- 回退状态下右键其他记录「回退」禁用（不允许嵌套回退，必须先恢复）
- 回退状态下恢复按钮可用性 = stash 匹配 && HEAD 未变

## 状态检测（零外部文件）

回退状态 = `git stash list` 第一条 stash 的 message 前缀 `keinestamp-rollback`。message 格式：

```
keinestamp-rollback <X-hash> <HEAD-hash>
```

解析 X（回退点，驱动灰显）与 HEAD（恢复条件）。不需要任何外部配置文件。

## 后端变更（src-tauri/src/lib.rs）

新命令（替换 3 个旧的）：

| 命令 | 说明 |
|---|---|
| `rollback(path, entryId)` | 回退到指定记录（stash + restore） |
| `get_rollback_state(path)` | `{ rollback_point: string\|null, head_matches: bool }` |
| `restore_rollback(path)` | stash pop（无冲突路径），错误类型区分冲突 |
| `get_rollback_conflicts(path)` | 冲突文件列表 |
| `resolve_rollback_conflicts(path, choices)` | 二选一写回 + add + drop |

删除：`revert`、`restore_revert`、`can_restore_revert`。

`app_config.rs`：删除 `LastRevert` 结构体、`last_revert` 字段、`get_last_revert`/`set_last_revert`。

`git_project.rs`：新增 `rollback`、`restore_rollback`、`get_rollback_state`、`get_rollback_conflicts`、`resolve_rollback_conflicts`（含 stash message 编解码、`git stash list` 解析、`git show` 写回）。

## CLI 变更（src/main.rs）

- `revert <path>` → `rollback <path> --entry <id>`（回退到指定记录）
- `restore <path>` → 新恢复语义（stash pop；冲突时打印冲突文件清单并提示 GUI 处理或手动处理）
- `record` 的撤回警告检查（原 main.rs:178 查 AppConfig last_revert）改为查 `get_rollback_state`
- 删除 `cmd_revert`/`cmd_restore` 旧逻辑

## 前端变更（frontend/src/）

- `App.svelte`：删除 `handleRevert`/`handleRestore` 与 ActionBar 相关 props；新增 rollback 状态轮询（并入 3s 轮询）；恢复确认对话框（5s 倒计时）；冲突解决弹窗
- `ActionBar.svelte`：删除 导出/撤回/恢复 三个按钮 → 剩 记录、部署
- `EntryList.svelte`：每条记录卡片右键菜单（导出 / 回退）；面板上侧新增恢复按钮；比回退点新的记录灰显（`.dimmed` 样式）；导出 = 原 exportEntry 逻辑传 entry id
- `CommitDialog.svelte`：restore-warn 复用（回退状态下显示，文案改"回退未恢复，继续记录将无法再恢复"）
- `api.ts`：删 `revert`/`restoreRevert`/`canRestoreRevert`，加 `rollback`/`getRollbackState`/`restoreRollback`/`getRollbackConflicts`/`resolveRollbackConflicts`
- `types.ts`：`RollbackState`、`RollbackConflict` 类型

## AGENTS.md git 安全约束更新

新增例外（本地、可逆、不碰历史指针）：

- `git stash push/pop` — 暂存/恢复工作区状态
- `git restore --source=<commit> --staged --worktree` — 文件回退（stash 兜底可逆）
- `git show <stash>:<path>` 写回文件 — 冲突二选一「最新版本」（与导出同类操作）

仍禁止：checkout / branch / merge / rebase / reset --hard / force push。

## 测试

- `tests/cli_tests.rs`：`test_init_then_revert` 改为 `test_rollback_and_restore`（init → 两次 record → rollback 到第一条 → 验证文件内容回到第一版 → restore → 验证文件回到最新）
- 新逻辑零全局状态（stash 在项目 .git 内）→ 测试污染问题根治，无需其他测试改造

## 验证

- `cargo test`
- `cd frontend; npm run check`
