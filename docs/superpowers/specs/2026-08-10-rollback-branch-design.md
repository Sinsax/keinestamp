# Rollback 分支方案重构设计

日期：2026-08-10
状态：已批准（2026-08-10 定稿，三个关键决策：记录=一步顶替 / 未提交=弹窗提交或放弃 / 约束按设计放开）

## 背景与动机

当前回退/恢复机制（stash + 自定义 refs + 手工三向合并 + 冲突二选一）经多轮补丁后功能完整（129 测试全绿，tag `v0.1.0-rollback-stable`），但复杂度集中在"合并"环节：

- 恢复 = 手工三向合并（base=回退点）+ 冲突二选一弹窗
- 补丁链：staged-only 检测、LFS smudge filter、无 stash 自动合并、skip-worktree 保留、untrack_ignored_files

探讨结论：问题集中在合并；合并根源 = 回退期间未提交工作。**分支方案**将回退/恢复改为 git 原生分支操作，恢复 = 纯切换零合并；代价是回退期间工作需"先提交或放弃"约定 + push 守护 + 约束放开。

## 目标

- 回退 = 创建回退分支 + 切换（git 原生）
- 恢复 = 切回原分支 + 删回退分支（零合并）
- 顶替 = 回退分支提交 + `merge --ff-only` 快进（原生）
- 删除：三向合并、二选一、stash、reset --soft hack
- 协作前瞻：回退分支命名规范、永不推送、提交即协作单元（多人协作留待将来）

## 核心机制

### 回退（右键记录 X → 回退，point = X^）

```
git checkout -b keinestamp/rollback <X^>          # 创建回退分支 + 切换，工作区 = X^ 状态
git update-ref refs/keinestamp/rollback-point <X^> # 灰显用（保留）
git update-ref refs/keinestamp/rollback <原分支名>  # 恢复/推送用（语义从"回退前 HEAD"改为"原分支名"）
```

- 回退前未提交修改：`checkout -b` 默认跟随切换（无冲突则带到回退分支继续，冲突则报错提示先处理）
- 回退分支名固定 `keinestamp/rollback`（单槽位）

### 回退状态下记录（一步完成顶替）

```
git add . + commit（父 = 回退点，提交到回退分支）
git checkout <原分支名>                    # 切回（回退分支干净 → 零合并）
git merge --ff-only keinestamp/rollback    # 快进原分支到新记录
git branch -D keinestamp/rollback          # 删回退分支
清 refs（rollback-point + rollback）
timeline 移除回退点之后记录 + append 新记录（现有逻辑）
web 重新生成 + web commit（现有逻辑）
```

- 原分支在回退期间有前进（外部提交）→ `merge --ff-only` 失败 → 报错提示（罕见，单用户场景 pull 已禁用）

### 恢复（放弃重做）

```
未提交修改检查（回退分支工作区 dirty）→ 弹「提交到回退分支 / 放弃修改 / 取消」：
  提交到回退分支 = git add . + commit「回退期间工作」
  放弃修改 = git restore --worktree -- .（或 checkout 丢弃，工作区操作）
git checkout <原分支名>                    # 零合并，工作区 = 回退前状态
git branch -D keinestamp/rollback
清 refs
untrack_ignored_files + reapply_skip_worktree（checkout 后状态修复）
```

- **回退后记录（在回退分支提交）不再破坏恢复**——恢复 = 切回原分支（原分支未动），回退分支的提交保留（可选快进）

### 连续回退

```
回退分支上再回退到更早点：git checkout -B keinestamp/rollback <new X^>
（-B = 强制移动分支 + 切换；切换前未提交修改按「提交或放弃」弹窗处理）
```

### 状态检测

```
get_rollback_state：
  当前分支 == keinestamp/rollback → 回退状态
  rollback_point = refs/keinestamp/rollback-point（灰显）
  head_matches 概念删除（恢复永远可用——原分支未动）
```

## git 安全约束更新

- **新增允许**（仅回退机制内部）：`git checkout -b <branch>` / `git checkout <branch>`、`git merge --ff-only`、`git branch -D`
- **仍禁止**：非快进 merge（内容合并）、reset --hard、rebase（除 pull --rebase）
- 保留现有例外：stash push/pop/drop、git restore --source、update-ref refs/keinestamp/*（回退机制内部已不再使用 stash/restore 三向合并，但 keep_rollback_file/untrack 仍用 restore）

## 推送 / 部署语义

- `push`：回退状态下显式推**原分支**（`git push origin <原分支名>`），回退分支永不推送
- `pull`：回退状态下禁用（报错「请先恢复回退」）
- 部署：web 仓库独立，不受影响

## 保留的现有机制

- `refs/keinestamp/rollback-point`（灰显）
- `untrack_ignored_files` / `check_ignore` / `rule_ignored` 灰显
- `keep_rollback_file`：内容源从 HEAD 改为**原分支**（回退分支 HEAD = 回退点）
- skip-worktree 保留（checkout 后 reapply）
- CLI/GUI 命令骨架

## 删除的机制

- `rollback_stash_files` / `rollback_local_files` / `stage_unmerged` / `find_rollback_stash`
- `resolve_rollback_conflicts` / `get_rollback_conflicts`（无合并冲突）
- stash 三向合并、无 stash 合并分支、reset --soft 顶替、连续回退自动恢复
- 前端 ConflictDialog / 相关 api / CONFLICT| 协议

## 前端变更

- `get_rollback_state` 返回结构：`{ rollback_point }`（head_matches 删除）
- 恢复流程：未提交 → 新弹窗「提交到回退分支 / 放弃修改 / 取消」
- 恢复按钮：回退状态即可用（无 head_matches 限制）
- 回退/导出右键菜单、灰显、忽略/保留文件：保留

## 测试

- 重写：rollback/restore/连续回退/顶替（分支语义）
- 新增：恢复零合并、记录后仍可恢复、快进顶替、未提交「提交或放弃」语义、push 守护

## 验证

- `cargo test` 全绿、`cd frontend; npm run check` 0/0
- 手动：回退 → 改文件 → 记录（一步顶替）/ 恢复（弹窗）→ 灰显/忽略/保留文件正常
