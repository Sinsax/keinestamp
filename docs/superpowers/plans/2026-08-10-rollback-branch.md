# Rollback 分支方案重构 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将回退/恢复机制从 stash+refs+手工三向合并重构为 git 分支方案（回退=checkout 分支、恢复=切回零合并、顶替=快进）。

**Architecture:** 回退 = `git checkout -b keinestamp/rollback <X^>` + refs（`rollback-point`=回退点灰显、`rollback`=原分支名）；恢复 = 未提交检查（`UNCOMMITTED|` 错误）→ `checkout <原分支>` → `branch -D`；记录顶替 = 回退分支 commit → `checkout` → `merge --ff-only` → `branch -D`；连续回退 = `checkout -B`。删除三向合并/二选一/stash/reset hack。

**Tech Stack:** Rust 2024（anyhow/clap/serde/tauri v2）、Svelte 5 + TypeScript。

## Global Constraints

- 回退分支名固定 `keinestamp/rollback`；refs 语义：`refs/keinestamp/rollback`=原分支名、`refs/keinestamp/rollback-point`=回退点
- git 安全新增例外（仅回退机制内部）：`git checkout -b`/`git checkout`/`git merge --ff-only`/`git branch -D`；仍禁止非快进 merge/reset --hard/rebase（除 pull --rebase）
- 回退状态下 push 只推原分支（从 `refs/keinestamp/rollback` 读）；pull 禁用
- 恢复前未提交修改 → 后端返回 `UNCOMMITTED|` 前缀错误，前端弹「提交到回退分支/放弃修改/取消」
- `RollbackState { is_rollback: bool, rollback_point: Option<String> }`（head_matches 删除）
- 验证：`cargo test`、`cd frontend; npm run check`
- 日志格式 `eprintln!("[debug] ...")` 沿用

---

### Task 1: git_project.rs 分支方案核心

**Files:**
- Modify: `src/git_project.rs`

**Interfaces:**
- Produces（后续任务依赖）:
  - `pub struct RollbackState { pub is_rollback: bool, pub rollback_point: Option<String> }`
  - `pub fn rollback(project_root: &Path, entry_hash: &str) -> Result<()>` — 回退到 entry 的父（首条记录=init；root 无父报错；连续回退=checkout -B；回退分支未提交时 bail）
  - `pub fn get_rollback_state(project_root: &Path) -> Result<RollbackState>`
  - `pub fn restore_rollback(project_root: &Path) -> Result<()>` — 未提交时 bail「回退分支有未提交修改，请先提交或放弃」
  - `pub fn commit_rollback_work(project_root: &Path) -> Result<()>` — 回退分支上 add+commit「回退期间工作」
  - `pub fn discard_rollback_changes(project_root: &Path) -> Result<()>` — 回退分支上 restore+clean 放弃
  - `pub fn finish_rollback_record(project_root: &Path, files: Option<&[String]>, message: &str) -> Result<String>` — 提交+快进+删分支+清 refs，返回新 hash
  - `pub fn clear_rollback(project_root: &Path) -> Result<()>` — 清 refs（若在回退分支则切回原分支+删分支）
  - `pub fn keep_rollback_file(project_root: &Path, path: &str) -> Result<()>` — 内容源改为原分支
  - 保留：`untrack_ignored_files`、`check_ignore`、`snapshot_skip_worktree`、`reapply_skip_worktree`
  - 删除：`find_rollback_stash`、`rollback_stash_files`、`rollback_local_files`、`stage_unmerged`、`get_rollback_conflicts`、`resolve_rollback_conflicts`、`reset_soft_to`、`get_rollback_point_hash`、`run_git_stdin`、`ROLLBACK_STASH_PREFIX`

- [ ] **Step 1: 常量与辅助改造**

```rust
const ROLLBACK_REF: &str = "refs/keinestamp/rollback";
const ROLLBACK_POINT_REF: &str = "refs/keinestamp/rollback-point";
const ROLLBACK_BRANCH: &str = "keinestamp/rollback";
```

删除 `ROLLBACK_STASH_PREFIX` 与 `find_rollback_stash`（106-130 行区域）。保留 `snapshot_skip_worktree`/`reapply_skip_worktree`（132-148 行）。

新增辅助（放在 `reapply_skip_worktree` 之后）：

```rust
fn worktree_dirty(project_root: &Path) -> Result<bool> {
    Ok(run_git_output(project_root, &["status", "--porcelain"])?
        .lines()
        .any(|l| !l.trim().is_empty()))
}

fn rollback_orig_branch(project_root: &Path) -> Result<String> {
    run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?
        .trim()
        .to_string()
        .pipe_ok()
}

trait PipeOk { fn pipe_ok(self) -> Self; }
impl PipeOk for String { fn pipe_ok(self) -> Self { self } }
```

（如不需要 trait 可省略，直接内联。）

- [ ] **Step 2: rollback + get_rollback_state**

```rust
pub fn rollback(project_root: &Path, entry_hash: &str) -> Result<()> {
    eprintln!("[debug] rollback: root={:?}, entry={}", project_root, entry_hash);
    run_git_output(project_root, &["rev-parse", "--verify", &format!("{}^0", entry_hash)])
        .map_err(|_| anyhow::anyhow!("记录不存在: {}", entry_hash))?;
    let point = run_git_output(project_root, &["rev-parse", "--verify", &format!("{}^", entry_hash)])
        .map(|s| s.trim().to_string())
        .map_err(|_| anyhow::anyhow!("第一条记录无法回退，没有更早的状态"))?;
    let current = get_current_branch(project_root)?;
    let orig = if current == ROLLBACK_BRANCH {
        if worktree_dirty(project_root)? {
            anyhow::bail!("回退分支有未提交修改，请先提交或放弃后再回退");
        }
        run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?
            .trim()
            .to_string()
    } else {
        current
    };
    if current == ROLLBACK_BRANCH {
        run_git(project_root, &["checkout", "-B", ROLLBACK_BRANCH, &point])?;
    } else {
        run_git(project_root, &["checkout", "-b", ROLLBACK_BRANCH, &point])?;
    }
    run_git(project_root, &["update-ref", ROLLBACK_POINT_REF, &point])?;
    run_git(project_root, &["update-ref", ROLLBACK_REF, &orig])?;
    eprintln!("[debug] rollback → ok (branch={}, point={})", ROLLBACK_BRANCH, point);
    Ok(())
}

pub struct RollbackState {
    pub is_rollback: bool,
    pub rollback_point: Option<String>,
}

pub fn get_rollback_state(project_root: &Path) -> Result<RollbackState> {
    let on_branch = get_current_branch(project_root)
        .map(|b| b == ROLLBACK_BRANCH)
        .unwrap_or(false);
    if !on_branch {
        return Ok(RollbackState { is_rollback: false, rollback_point: None });
    }
    let point = run_git_output_quiet(project_root, &["rev-parse", "--verify", ROLLBACK_POINT_REF])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(RollbackState { is_rollback: true, rollback_point: point })
}
```

`get_current_branch` 需改为 `pub`（当前 470 行附近是 `fn get_current_branch`）。

- [ ] **Step 3: restore_rollback + commit/discard + finish_rollback_record**

```rust
pub fn restore_rollback(project_root: &Path) -> Result<()> {
    eprintln!("[debug] restore_rollback: root={:?}", project_root);
    let orig = rollback_orig_branch(project_root)?;
    if worktree_dirty(project_root)? {
        anyhow::bail!("回退分支有未提交修改，请先提交或放弃后再恢复");
    }
    let skip = snapshot_skip_worktree(project_root);
    run_git(project_root, &["checkout", &orig])?;
    run_git(project_root, &["branch", "-D", ROLLBACK_BRANCH])?;
    reapply_skip_worktree(project_root, &skip)?;
    untrack_ignored_files(project_root)?;
    clear_rollback_refs(project_root)?;
    eprintln!("[debug] restore_rollback → ok");
    Ok(())
}

pub fn commit_rollback_work(project_root: &Path) -> Result<()> {
    eprintln!("[debug] commit_rollback_work: root={:?}", project_root);
    if get_current_branch(project_root)? != ROLLBACK_BRANCH {
        anyhow::bail!("不在回退状态");
    }
    ensure_git_user(project_root)?;
    run_git(project_root, &["add", "."])?;
    run_git(project_root, &["commit", "-m", "回退期间工作"])?;
    Ok(())
}

pub fn discard_rollback_changes(project_root: &Path) -> Result<()> {
    eprintln!("[debug] discard_rollback_changes: root={:?}", project_root);
    if get_current_branch(project_root)? != ROLLBACK_BRANCH {
        anyhow::bail!("不在回退状态");
    }
    run_git(project_root, &["restore", "--staged", "--worktree", "--", "."])?;
    run_git(project_root, &["clean", "-fd"])?;
    Ok(())
}

pub fn finish_rollback_record(
    project_root: &Path,
    files: Option<&[String]>,
    message: &str,
) -> Result<String> {
    eprintln!("[debug] finish_rollback_record: root={:?}, message={:?}", project_root, message);
    if get_current_branch(project_root)? != ROLLBACK_BRANCH {
        anyhow::bail!("不在回退状态");
    }
    ensure_git_user(project_root)?;
    lfs_auto_configure(project_root)?;
    let hash = match files {
        Some(f) => add_and_commit_files(project_root, f, message)?,
        None => add_and_commit(project_root, message)?,
    };
    let orig = rollback_orig_branch(project_root)?;
    run_git(project_root, &["checkout", &orig])?;
    run_git(project_root, &["merge", "--ff-only", ROLLBACK_BRANCH])?;
    run_git(project_root, &["branch", "-D", ROLLBACK_BRANCH])?;
    clear_rollback_refs(project_root)?;
    eprintln!("[debug] finish_rollback_record → ok, hash={}", hash);
    Ok(hash)
}
```

- [ ] **Step 4: clear_rollback + keep_rollback_file 改造**

```rust
pub fn clear_rollback(project_root: &Path) -> Result<()> {
    if get_current_branch(project_root).map(|b| b == ROLLBACK_BRANCH).unwrap_or(false) {
        if let Ok(orig) = rollback_orig_branch(project_root) {
            let _ = run_git(project_root, &["checkout", &orig]);
            let _ = run_git(project_root, &["branch", "-D", ROLLBACK_BRANCH]);
        }
    }
    clear_rollback_refs(project_root)
}

pub fn keep_rollback_file(project_root: &Path, path: &str) -> Result<()> {
    eprintln!("[debug] keep_rollback_file: root={:?}, path={}", project_root, path);
    let st = get_rollback_state(project_root)?;
    if !st.is_rollback {
        anyhow::bail!("不在回退状态，无法保留文件");
    }
    let orig = rollback_orig_branch(project_root)?;
    run_git_output(project_root, &["rev-parse", "--verify", &format!("{}:{}", orig, path)])
        .map_err(|_| anyhow::anyhow!("文件在回退前状态不存在，无需保留"))?;
    let in_index = run_git_output(project_root, &["ls-files", "--error-unmatch", path]).is_ok();
    if in_index {
        anyhow::bail!("文件已在项目中，无需保留");
    }
    run_git(project_root, &["restore", "--source", &orig, "--staged", "--worktree", "--", path])?;
    eprintln!("[debug] keep_rollback_file → ok");
    Ok(())
}
```

- [ ] **Step 5: 删除旧函数**

删除：`rollback_stash_files`、`rollback_local_files`、`stage_unmerged`、`restore_rollback` 旧三向合并实现（已替换）、`get_rollback_conflicts`、`resolve_rollback_conflicts`、`reset_soft_to`、`get_rollback_point_hash`、`run_git_stdin`、`run_git_stdin_output`（若 `check_ignore` 已自带 spawn 则删除该辅助；`check_ignore` 保留）。

- [ ] **Step 6: 单元测试重写（mod tests 内）**

删除全部旧 rollback 测试（`test_rollback_roundtrip`、`test_rollback_conflict_resolve`、`test_rollback_roundtrip_chinese`、`test_rollback_conflict_resolve_chinese`、`test_resolve_rollback_conflicts_incomplete_choices`、`test_rollback_skip_worktree_preserved_after_restore`、`test_rollback_no_stash_*`、`test_rollback_staged_change_conflicts`、`test_rollback_conflict_resolve_latest_deletes_absent_in_stash`、`test_find_rollback_stash_exact_match`、`test_rollback_invalid_hash_leaves_no_stash`、`test_rollback_while_in_rollback_auto_restores`、`test_rollback_identical_edits_no_conflict`、`test_rollback_both_deleted_no_conflict`、`test_rollback_first_entry_rejected`、`test_keep_rollback_file`、`test_keep_rollback_file_outside_rollback`、`test_restore_untracks_ignored_files`、`test_check_ignore_matches_tracked` — 保留 `test_check_ignore_matches_tracked`），新增：

```rust
#[test]
fn test_rollback_roundtrip_branch() -> Result<()> {
    let dir = tmp_repo("rb_branch_roundtrip");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("a.txt"), b"v2")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;
    let orig_branch = get_current_branch(&dir)?;

    rollback(&dir, &second)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
    assert_eq!(get_current_branch(&dir)?, ROLLBACK_BRANCH, "应在回退分支");
    let st = get_rollback_state(&dir)?;
    assert!(st.is_rollback);
    assert!(st.rollback_point.is_some());

    restore_rollback(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2");
    assert_eq!(get_current_branch(&dir)?, orig_branch, "应切回原分支");
    let st = get_rollback_state(&dir)?;
    assert!(!st.is_rollback);

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_rollback_finish_record_replaces() -> Result<()> {
    let dir = tmp_repo("rb_branch_finish");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("a.txt"), b"v2")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
    std::fs::write(dir.join("a.txt"), b"v3")?;
    let hash = finish_rollback_record(&dir, None, "C")?;
    assert_eq!(get_current_branch(&dir)?, "master");
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3");
    let log_out = log(&dir, 10)?;
    assert!(!log_out.contains("second"), "被回退记录应消失: {}", log_out);
    assert!(log_out.contains("C"));
    let st = get_rollback_state(&dir)?;
    assert!(!st.is_rollback);

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_restore_uncommitted_blocks_and_commit_work() -> Result<()> {
    let dir = tmp_repo("rb_uncommitted");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("a.txt"), b"v2")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    std::fs::write(dir.join("a.txt"), b"work")?;
    let err = restore_rollback(&dir).expect_err("未提交时应拒绝恢复");
    assert!(err.to_string().contains("未提交修改"), "err: {}", err);
    assert_eq!(get_current_branch(&dir)?, ROLLBACK_BRANCH);

    commit_rollback_work(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"work");
    restore_rollback(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2", "恢复后回退期间提交的工作不保留（原分支未动）");

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_discard_rollback_changes() -> Result<()> {
    let dir = tmp_repo("rb_discard");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("a.txt"), b"v2")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    std::fs::write(dir.join("a.txt"), b"work")?;
    std::fs::write(dir.join("new.txt"), b"new")?;
    discard_rollback_changes(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1", "修改应放弃");
    assert!(!dir.join("new.txt").exists(), "未跟踪文件应删除");
    restore_rollback(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2");

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_rollback_continuous() -> Result<()> {
    let dir = tmp_repo("rb_continuous");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    let first = get_current_commit_hash(&dir)?;
    std::fs::write(dir.join("a.txt"), b"v2")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
    // 连续回退到第一条记录（init 状态 = 空，只有配置文件？tmp_repo 无 .gitignore——回退 first 前 = 空树？first 是 root commit！）
    let err = rollback(&dir, &first).expect_err("root commit 无父应报错");
    assert!(err.to_string().contains("第一条记录无法回退"), "err: {}", err);
    restore_rollback(&dir)?;
    assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2");

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_keep_rollback_file_branch() -> Result<()> {
    let dir = tmp_repo("rb_keep_branch");
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("b.txt"), b"b1")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    assert!(!dir.join("b.txt").exists());
    keep_rollback_file(&dir, "b.txt")?;
    assert_eq!(std::fs::read(dir.join("b.txt"))?, b"b1");
    assert!(run_git_output(&dir, &["ls-files", "--error-unmatch", "b.txt"]).is_ok());

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_restore_untracks_ignored_files_branch() -> Result<()> {
    let dir = tmp_repo("rb_untrack_branch");
    std::fs::write(dir.join(".gitignore"), b"out/\n")?;
    std::fs::write(dir.join("a.txt"), b"v1")?;
    add_and_commit(&dir, "first")?;
    std::fs::write(dir.join("x.txt"), b"x1")?;
    add_and_commit(&dir, "second")?;
    let second = get_current_commit_hash(&dir)?;

    rollback(&dir, &second)?;
    let mut gi = std::fs::read_to_string(dir.join(".gitignore"))?;
    gi.push_str("x.txt\n");
    std::fs::write(dir.join(".gitignore"), &gi)?;
    restore_rollback(&dir)?;
    assert_eq!(std::fs::read(dir.join("x.txt"))?, b"x1");
    let tracked = tracked_files(&dir)?;
    assert!(!tracked.iter().any(|t| t == "x.txt"), "应取消跟踪: {:?}", tracked);

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
```

注意 `test_rollback_finish_record_replaces` 中断言分支名 "master" — 用 `tmp_repo` 的默认分支（git init 默认 master；若环境默认 main 则断言用实际 orig）。改为先取 `let orig = get_current_branch(&dir)?;` 再断言。

- [ ] **Step 7: 运行测试**

Run: `cargo test --lib git_project`
Expected: 全部 PASS（旧测试已删，新增 7 个）。

- [ ] **Step 8: Commit**

```bash
git add src/git_project.rs
git commit -m "refactor(rollback): 分支方案核心（checkout 回退分支/切回恢复/快进顶替）"
```

---

### Task 2: CLI 改造（main.rs）

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: Task 1 全部
- Produces: `rollback <path> --entry <id>`（连续回退支持）、`restore <path>`（未提交提示）、`record`（回退状态顶替）、`push`（回退状态推原分支）、`pull`（回退状态报错）

- [ ] **Step 1: cmd_record 顶替路径**

`cmd_record` 中替换现有 rollback_point/reset_soft_to 逻辑（main.rs 178-200 行区域）：

```rust
let rollback_state = keinestamp::git_project::get_rollback_state(&meta.project_root)?;
let rollback_point = if rollback_state.is_rollback {
    if rollback_state.rollback_point.is_some() {
        eprintln!("⚠️ 注意：当前处于回退状态，记录将顶替被回退的记录（原记录被新记录取代）");
    }
    rollback_state.rollback_point.clone()
} else {
    None
};
```

`add_and_commit` 调用改为：

```rust
let hash = if rollback_state.is_rollback {
    keinestamp::git_project::finish_rollback_record(&meta.project_root, None, message)?
} else {
    keinestamp::git_project::add_and_commit(&meta.project_root, message)?
};
```

（CLI record 无 files 参数，传 None。）`remove_entries_after` 保留（rollback_point Some 时）。

- [ ] **Step 2: cmd_restore 未提交提示**

`cmd_restore`（main.rs 336-348 区域）改为：

```rust
fn cmd_restore(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let st = keinestamp::git_project::get_rollback_state(&meta.project_root)?;
    if !st.is_rollback {
        anyhow::bail!("没有可恢复的回退");
    }
    match keinestamp::git_project::restore_rollback(&meta.project_root) {
        Ok(_) => {
            println!("已恢复回退前的状态");
            Ok(())
        }
        Err(e) if e.to_string().contains("未提交修改") => {
            println!("回退分支有未提交修改，无法直接恢复：");
            println!("  - 在 GUI 中弹窗选择「提交到回退分支」或「放弃修改」");
            println!("  - 或手动执行：git add . && git commit -m 回退期间工作（提交） / git restore . && git clean -fd（放弃）");
            Ok(())
        }
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 3: cmd_push / cmd_pull 守护**

`cmd_push` 开头加：

```rust
let st = keinestamp::git_project::get_rollback_state(&meta.project_root)?;
if st.is_rollback {
    let orig = keinestamp::git_project::rollback_orig_branch_pub(&meta.project_root)?;
    keinestamp::git_project::push_branch(&meta.project_root, &orig)?;
    println!("已推送到远端（原分支 {}）", orig);
    return Ok(());
}
```

（需在 git_project.rs 暴露：`pub fn rollback_orig_branch_pub` = 现有 `rollback_orig_branch` 改 pub；`push_branch(project_root, branch)` = `git push -u origin <branch>`。）

`cmd_pull` 开头加：

```rust
let st = keinestamp::git_project::get_rollback_state(&meta.project_root)?;
if st.is_rollback {
    anyhow::bail!("回退状态下无法拉取，请先恢复回退");
}
```

- [ ] **Step 4: git_project.rs 补充 pub**

```rust
pub fn rollback_orig_branch(project_root: &Path) -> Result<String> { ... }  // 原 fn 改 pub

pub fn push_branch(project_root: &Path, branch: &str) -> Result<()> {
    run_git(project_root, &["push", "-u", "origin", branch])
}
```

- [ ] **Step 5: 编译验证**

Run: `cargo build --bin keinestamp`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/git_project.rs
git commit -m "feat(cli): 分支方案 CLI（记录顶替/恢复未提交提示/push 原分支/pull 守护）"
```

---

### Task 3: Tauri 命令（lib.rs）

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1/2
- Produces（前端依赖）:
  - `rollback(path, entry_id)` — 连续回退支持
  - `get_rollback_state(path)` → `{"is_rollback": bool, "rollback_point": string|null}`
  - `restore_rollback(path)` — 未提交时返回 `UNCOMMITTED|` 前缀错误
  - `commit_rollback_work(path)` / `discard_rollback_changes(path)`
  - 删除：`get_rollback_conflicts`、`resolve_rollback_conflicts`
  - `record` 回退状态顶替；`push`/`pull` 守护；`keep_rollback_file` 不变

- [ ] **Step 1: rollback / get_rollback_state / restore_rollback 改造**

```rust
#[tauri::command]
async fn rollback(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] rollback({}, {})", path, &entry_id[..7.min(entry_id.len())]);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        map_err(keinestamp::git_project::rollback(&meta.project_root, &entry_id))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_state(path: String) -> Result<serde_json::Value, String> {
    eprintln!("[debug] get_rollback_state({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let st = map_err(keinestamp::git_project::get_rollback_state(&meta.project_root))?;
    Ok(serde_json::json!({
        "is_rollback": st.is_rollback,
        "rollback_point": st.rollback_point,
    }))
}

#[tauri::command]
async fn restore_rollback(path: String) -> Result<(), String> {
    eprintln!("[debug] restore_rollback({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        match keinestamp::git_project::restore_rollback(&meta.project_root) {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("未提交修改") => Err("UNCOMMITTED|".to_string()),
            Err(e) => Err(e.to_string()),
        }
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn commit_rollback_work(path: String) -> Result<(), String> {
    eprintln!("[debug] commit_rollback_work({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        map_err(keinestamp::git_project::commit_rollback_work(&meta.project_root))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn discard_rollback_changes(path: String) -> Result<(), String> {
    eprintln!("[debug] discard_rollback_changes({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        map_err(keinestamp::git_project::discard_rollback_changes(&meta.project_root))
    });
    handle.await.map_err(|e| e.to_string())?
}
```

- [ ] **Step 2: 删除 get_rollback_conflicts / resolve_rollback_conflicts**

删除两个命令函数 + invoke_handler 中对应条目；`restore_rollback` 的 CONFLICT 逻辑已由 Step 1 替换。

- [ ] **Step 3: record 顶替**

`record` 命令（lib.rs 70-110 区域）：替换现有 rollback_point/reset_soft_to 逻辑为：

```rust
let rollback_state = map_err(keinestamp::git_project::get_rollback_state(&meta.project_root))?;
let rollback_point = if rollback_state.is_rollback {
    rollback_state.rollback_point.clone()
} else {
    None
};
let hash: String = if rollback_state.is_rollback {
    map_err(keinestamp::git_project::finish_rollback_record(
        &meta.project_root,
        files.as_deref(),
        &message,
    ))?
} else {
    match files {
        Some(f) => map_err(keinestamp::git_project::add_and_commit_files(&meta.project_root, &f, &message))?,
        None => map_err(keinestamp::git_project::add_and_commit(&meta.project_root, &message))?,
    }
};
if let Some(ref point) = rollback_point {
    map_err(keinestamp::timeline::remove_entries_after(&meta.web_root.join("timeline.json"), point))?;
}
let _ = keinestamp::git_project::clear_rollback(&meta.project_root);
```

- [ ] **Step 4: push / pull 守护**

`push` 命令（lib.rs 320-340 区域）改：

```rust
let st = map_err(keinestamp::git_project::get_rollback_state(&meta.project_root))?;
if st.is_rollback {
    let orig = map_err(keinestamp::git_project::rollback_orig_branch(&meta.project_root))?;
    map_err(keinestamp::git_project::push_branch(&meta.project_root, &orig))?;
} else {
    map_err(keinestamp::git_project::push(&meta.project_root))?;
}
```

`pull` 命令开头加回退状态检查：

```rust
let st = map_err(keinestamp::git_project::get_rollback_state(&meta.project_root))?;
if st.is_rollback {
    return Err("回退状态下无法拉取，请先恢复回退".into());
}
```

- [ ] **Step 5: 编译验证**

Run: `cd src-tauri; cargo check`
Expected: 编译通过，无未使用警告。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src/git_project.rs
git commit -m "feat(tauri): 分支方案命令（UNCOMMITTED 协议/commit-discard 工作/record 顶替/push-pull 守护）"
```

---

### Task 4: 前端改造

**Files:**
- Modify: `frontend/src/types.ts`、`frontend/src/api.ts`、`frontend/src/App.svelte`、`frontend/src/lib/EntryList.svelte`、`frontend/src/lib/CommitDialog.svelte`
- Create: `frontend/src/lib/RollbackChangesDialog.svelte`
- Delete: `frontend/src/lib/ConflictDialog.svelte`

**Interfaces:**
- Consumes: Task 3 命令
- Produces: 完整 GUI（恢复弹窗「提交到回退分支/放弃修改/取消」）

- [ ] **Step 1: types.ts**

```ts
export interface RollbackState {
  is_rollback: boolean
  rollback_point: string | null
}
```

删除 `RollbackChoice` 类型（若仅 ConflictDialog 使用）。

- [ ] **Step 2: api.ts**

删除 `getRollbackConflicts`、`resolveRollbackConflicts`；新增：

```ts
export async function commitRollbackWork(path: string): Promise<void> {
  return invoke<void>('commit_rollback_work', { path })
}

export async function discardRollbackChanges(path: string): Promise<void> {
  return invoke<void>('discard_rollback_changes', { path })
}
```

- [ ] **Step 3: 创建 RollbackChangesDialog.svelte**

```svelte
<script lang="ts">
  let {
    busy,
    onCommit,
    onDiscard,
    onCancel,
  }: {
    busy: boolean
    onCommit: () => void
    onDiscard: () => void
    onCancel: () => void
  } = $props()
</script>

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog rollback-changes-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>回退期间有未提交修改</h3>
    <p class="changes-hint">恢复回退前需要先处理回退分支上的未提交修改：</p>
    <div class="actions">
      <button onclick={onCancel} disabled={busy}>取消</button>
      <button onclick={onDiscard} disabled={busy}>放弃修改</button>
      <button class="accent" onclick={onCommit} disabled={busy}>
        {busy ? '处理中...' : '提交到回退分支'}
      </button>
    </div>
  </div>
</div>

<style>
  .changes-hint {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 14px;
  }
</style>
```

- [ ] **Step 4: App.svelte 改造**

删除：`ConflictDialog` import、`conflictFiles`/`resolvingConflict` 状态、`loadConflictFiles`、`handleResolveConflict`、`handleRestore` 的 CONFLICT 分支、`handleRollback` 的 CONFLICT 分支。

新增状态与处理：

```ts
let showRollbackChanges = $state(false)
let rollbackChangesBusy = $state(false)

const canRestore = $derived(rollbackState.is_rollback)
```

`handleRestore` 改为：

```ts
async function handleRestore() {
  if (!activePath) return
  rollbackBusy = true
  try {
    await api.restoreRollback(activePath)
    pushToast('已恢复回退前的状态', 'success')
    await loadProject(activePath)
    await refreshRollbackState()
  } catch (e: any) {
    const msg = String(e)
    if (msg.startsWith('UNCOMMITTED|')) {
      showRollbackChanges = true
    } else {
      pushToast(`恢复失败: ${msg}`, 'error')
    }
  }
  rollbackBusy = false
}

async function handleRollbackCommitWork() {
  if (!activePath) return
  rollbackChangesBusy = true
  try {
    await api.commitRollbackWork(activePath)
    showRollbackChanges = false
    await handleRestore()
  } catch (e: any) {
    pushToast(`提交失败: ${e}`, 'error')
  }
  rollbackChangesBusy = false
}

async function handleRollbackDiscard() {
  if (!activePath) return
  rollbackChangesBusy = true
  try {
    await api.discardRollbackChanges(activePath)
    showRollbackChanges = false
    await handleRestore()
  } catch (e: any) {
    pushToast(`放弃失败: ${e}`, 'error')
  }
  rollbackChangesBusy = false
}
```

`handleRollback` 的 toast 逻辑保留（wasRolledBack 判断）；删除 CONFLICT 分支（catch 直接 toast）。

`rollbackState` 初始值改为 `{ is_rollback: false, rollback_point: null }`；`loadProject` 中的重置同步。

模板：删除 `{#if conflictFiles}` 块；新增：

```svelte
{#if showRollbackChanges}
  <RollbackChangesDialog
    busy={rollbackChangesBusy}
    onCommit={handleRollbackCommitWork}
    onDiscard={handleRollbackDiscard}
    onCancel={() => (showRollbackChanges = false)}
  />
{/if}
```

ActionBar 恢复按钮：`restoreEnabled={rollbackState.is_rollback}`、title 逻辑改 `!rollbackState.is_rollback ? '没有可恢复的回退' : '恢复回退前的状态'`（head_matches 分支删除）。

CommitDialog 的 `canRestore` 传 `rollbackState.is_rollback`（CommitDialog 内文案「回退状态下记录将顶替被回退的记录」保留）。

- [ ] **Step 5: EntryList.svelte**

回退按钮 disabled 条件 `rollbackBusy`（已实现）；title「将先恢复当前回退，再回退到该记录」→「回退状态下将切换到更早的回退点」。其余不变。

- [ ] **Step 6: 删除 ConflictDialog.svelte**

`Remove-Item frontend/src/lib/ConflictDialog.svelte`

- [ ] **Step 7: 前端检查**

Run: `cd frontend; npm run check`
Expected: 0 errors 0 warnings。

- [ ] **Step 8: Commit**

```bash
git add frontend/src/types.ts frontend/src/api.ts frontend/src/App.svelte frontend/src/lib/EntryList.svelte frontend/src/lib/CommitDialog.svelte frontend/src/lib/RollbackChangesDialog.svelte frontend/src/lib/ConflictDialog.svelte
git commit -m "feat(ui): 分支方案前端（恢复弹窗提交或放弃，删除冲突二选一）"
```

---

### Task 5: 集成测试重写（tests/cli_tests.rs）

**Files:**
- Modify: `tests/cli_tests.rs`

**Interfaces:**
- Consumes: Task 2 CLI

- [ ] **Step 1: 重写测试**

`test_cli_rollback_and_restore` 保留（回退→文件 v1→恢复→v2，行为兼容——验证分支切换语义）。`test_cli_rollback_then_record_replaces` 保留（顶替）。`test_cli_rollback_first_entry_to_init` 保留（首条回退到 init）。删除 `test_cli_rollback_while_in_rollback_auto_restores`（连续回退现在要求未提交先处理——改为新语义）、`test_cli_rollback_conflict_aborts_chain`（冲突场景删除）。

新增：

```rust
#[test]
fn test_cli_rollback_uncommitted_blocks_restore() {
    let dir = tmp_dir("cli_rb_uncommitted");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);
    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&dir.join("web-timeline").join("timeline.json")).unwrap();
    let b_id = timeline.entries[1].id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback failed: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v1");

    std::fs::write(dir.join("test.blend"), b"work").unwrap();
    let (out, ok) = run(&["restore", &dir.to_string_lossy()]);
    assert!(ok, "CLI 未提交时提示后返回 Ok: {}", out);
    assert!(out.contains("未提交修改"), "应提示未提交: {}", out);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_finish_record_replaces() {
    let dir = tmp_dir("cli_rb_finish");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);
    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    let tl_path = dir.join("web-timeline").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    let b_id = timeline.entries[1].id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "C"]);
    assert!(ok, "record C failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    assert_eq!(timeline.entries.len(), 2, "B 应被顶替: {:?}", timeline.entries.iter().map(|e| &e.message).collect::<Vec<_>>());
    assert_eq!(timeline.entries[1].message, "C");

    let log_out = keinestamp::git_project::log(&dir, 10).unwrap();
    assert!(!log_out.contains("B"), "git 历史不应再有 B: {}", log_out);
    assert_eq!(log_out.lines().count(), 3, "init → A → C: {}", log_out);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_push_rollback_guards() {
    let dir = tmp_dir("cli_rb_push");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);
    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);
    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&dir.join("web-timeline").join("timeline.json")).unwrap();
    let b_id = timeline.entries[1].id.clone();
    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback failed: {}", out);

    let (out, ok) = run(&["pull", &dir.to_string_lossy()]);
    assert!(!ok, "回退状态下 pull 应报错: {}", out);
    assert!(out.contains("回退状态下无法拉取") || out.is_empty(), "pull 报错: {}", out);

    std::fs::remove_dir_all(&dir).unwrap();
}
```

注意 `test_cli_push_rollback_guards` 的 pull 错误走 stderr（run 只捕获 stdout）→ 断言 `!ok` 即可，去掉 stdout 断言。

- [ ] **Step 2: 运行测试**

Run: `cargo test --test cli_tests`
Expected: 全部 PASS。

- [ ] **Step 3: Commit**

```bash
git add tests/cli_tests.rs
git commit -m "test(cli): 分支方案集成测试（未提交提示/快进顶替/pull 守护）"
```

---

### Task 6: AGENTS.md + 全量验证

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: AGENTS.md 更新**

1. **Git safety**：约束追加例外 `git checkout -b/checkout`（回退分支切换）、`git merge --ff-only`（快进顶替）、`git branch -D`（删回退分支），注明仅回退机制内部；仍禁止非快进 merge/reset --hard/rebase。
2. **rollback mechanism** 段整体替换为分支方案描述：回退 = `checkout -b keinestamp/rollback <记录^>`（首条=init，root 无父报错；连续回退 = `checkout -B`）；状态 = 当前分支 + `refs/keinestamp/rollback`（原分支名）+ `rollback-point`（回退点灰显）；恢复 = 未提交检查（`UNCOMMITTED|` 协议）→ `checkout <原分支>` → `branch -D` → untrack_ignored_files + skip-worktree 保留；记录顶替 = 回退分支 commit → `checkout` → `merge --ff-only` → `branch -D` → timeline 移除其后记录；回退状态下 push 推原分支、pull 禁用；协作前瞻（回退分支命名规范、永不推送、提交即协作单元）。
3. **Tauri 命令表**：删 `get_rollback_conflicts`/`resolve_rollback_conflicts` 行；`get_rollback_state` 改为 `{is_rollback, rollback_point}`；`restore_rollback` 描述改 `UNCOMMITTED|`；新增 `commit_rollback_work`/`discard_rollback_changes` 行；`rollback` 行描述更新（分支切换 + 连续回退）。
4. **前端组件**：删除 ConflictDialog 提及；App.svelte 恢复弹窗（提交或放弃）；EntryList 回退菜单连续回退。

- [ ] **Step 2: 全量验证**

Run: `cargo test` — 全部 PASS
Run: `cd frontend; npm run check` — 0 errors 0 warnings
Run: `cargo build --bin keinestamp`、`cd src-tauri; cargo check` — 通过

- [ ] **Step 3: grep 残留检查**

Run: `git grep -n "resolve_rollback_conflicts\|get_rollback_conflicts\|ConflictDialog\|head_matches\|ROLLBACK_STASH_PREFIX\|reset_soft_to\|find_rollback_stash"` — 无输出（除历史文档）。

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md
git commit -m "docs: 分支方案机制文档（git 安全例外 + rollback 描述 + 命令表 + 前端说明）"
```

---

## Self-Review 备注

- Spec 覆盖：回退/恢复/连续回退/顶替/未提交弹窗/push-pull 守护/约束放开/删除合并机制 全部有对应任务（Task 1-6）。
- 行为变化（需用户知晓）：恢复前未提交 → 弹窗；回退前未提交跟随切换（checkout -b 原生）；回退后记录不再破坏恢复；git 工具可见 keinestamp/rollback 分支。
- 测试策略：单元测试重写为分支语义；集成测试删除冲突场景，新增未提交/顶替/pull 守护。
