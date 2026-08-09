use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn add_and_commit(project_root: &Path, message: &str) -> Result<String> {
    eprintln!("[debug] add_and_commit: root={:?}, message={:?}", project_root, message);
    run_git(project_root, &["add", "."])?;

    run_git(project_root, &["commit", "-m", message])?;

    let hash = get_current_commit_hash(project_root)?;
    eprintln!("[debug] add_and_commit → hash={}", hash);
    Ok(hash)
}

pub fn add_and_commit_files(project_root: &Path, files: &[String], message: &str) -> Result<String> {
    eprintln!("[debug] add_and_commit_files: root={:?}, files={:?}, message={:?}", project_root, files, message);
    if files.is_empty() {
        anyhow::bail!("未选择要提交的文件");
    }
    let mut args = vec!["add", "--"];
    args.extend(files.iter().map(|f| f.as_str()));
    run_git(project_root, &args)?;

    let mut commit_args = vec!["commit", "-m", message, "--"];
    commit_args.extend(files.iter().map(|f| f.as_str()));
    run_git(project_root, &commit_args)?;

    let hash = get_current_commit_hash(project_root)?;
    eprintln!("[debug] add_and_commit_files → hash={}", hash);
    Ok(hash)
}

pub fn stage_files_and_commit(project_root: &Path, files: &[&str], message: &str) -> Result<String> {
    eprintln!("[debug] stage_files_and_commit: root={:?}, files={:?}, message={:?}", project_root, files, message);
    let mut args = vec!["add", "--force"];
    args.extend_from_slice(files);
    run_git(project_root, &args)?;
    run_git(project_root, &["commit", "-m", message])?;
    let hash = get_current_commit_hash(project_root)?;
    eprintln!("[debug] stage_files_and_commit → hash={}", hash);
    Ok(hash)
}

pub fn push(project_root: &Path) -> Result<()> {
    eprintln!("[debug] push: root={:?}", project_root);
    let branch = get_current_branch(project_root)?;
    eprintln!("[debug] push: branch={}", branch);

    let has_origin = run_git_output(project_root, &["remote"])
        .map(|s| s.lines().any(|l| l.trim() == "origin"))
        .unwrap_or(false);
    if !has_origin {
        anyhow::bail!("没有名为 origin 的 git 远端。请先用 `git remote add origin <url>` 添加。");
    }

    run_git(project_root, &["push", "-u", "origin", &branch])?;
    eprintln!("[debug] push → ok");
    Ok(())
}

pub fn pull(project_root: &Path) -> Result<()> {
    eprintln!("[debug] pull: root={:?}", project_root);
    run_git(project_root, &["pull", "--rebase"])?;
    eprintln!("[debug] pull → ok");
    Ok(())
}

pub fn has_remote(project_root: &Path) -> bool {
    let output = run_git_output(project_root, &["remote"]);
    match output {
        Ok(s) => s.lines().any(|l| l.trim() == "origin"),
        Err(_) => false,
    }
}

pub fn status(project_root: &Path) -> Result<String> {
    let output = run_git_output(project_root, &[
        "-c", "core.quotepath=false",
        "status", "--porcelain", "-uall",
    ])?;
    Ok(output)
}

pub fn log(project_root: &Path, max_count: usize) -> Result<String> {
    let output = run_git_output(
        project_root,
        &[
            "log",
            &format!("--max-count={}", max_count),
            "--format=%H|%ai|%s",
        ],
    )?;
    Ok(output)
}

pub fn get_current_commit_hash(project_root: &Path) -> Result<String> {
    let hash = run_git_output(project_root, &["rev-parse", "HEAD"])?;
    Ok(hash.trim().to_string())
}

const ROLLBACK_REF: &str = "refs/keinestamp/rollback";
const ROLLBACK_POINT_REF: &str = "refs/keinestamp/rollback-point";
const ROLLBACK_STASH_PREFIX: &str = "keinestamp-rollback";

fn find_rollback_stash(project_root: &Path) -> Result<Option<String>> {
    let out = run_git_output(project_root, &["stash", "list", "--format=%gd %s"])?;
    Ok(out
        .lines()
        .find_map(|line| {
            let mut it = line.splitn(2, ' ');
            let refname = it.next()?;
            let msg = it.next().unwrap_or("");
            if msg
                .split_once(": ")
                .map(|(_, m)| m == ROLLBACK_STASH_PREFIX)
                .unwrap_or(false)
            {
                Some(refname.to_string())
            } else {
                None
            }
        }))
}

fn clear_rollback_refs(project_root: &Path) -> Result<()> {
    let _ = run_git(project_root, &["update-ref", "-d", ROLLBACK_REF]);
    let _ = run_git(project_root, &["update-ref", "-d", ROLLBACK_POINT_REF]);
    Ok(())
}

fn snapshot_skip_worktree(project_root: &Path) -> Vec<String> {
    run_git_output(project_root, &["-c", "core.quotepath=false", "ls-files", "-v"])
        .map(|out| {
            out.lines()
                .filter_map(|l| l.strip_prefix('S').map(|s| s.trim_start().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn reapply_skip_worktree(project_root: &Path, paths: &[String]) -> Result<()> {
    for p in paths {
        let _ = run_git(project_root, &["update-index", "--skip-worktree", p]);
    }
    Ok(())
}

pub fn rollback(project_root: &Path, entry_hash: &str) -> Result<()> {
    eprintln!("[debug] rollback: root={:?}, entry={}", project_root, entry_hash);
    if get_rollback_state(project_root)?.rollback_point.is_some() {
        // 回退状态下再次回退 = 先自动恢复（回到回退前），再回退到新目标
        eprintln!("[debug] rollback: in rollback state, auto-restore first");
        restore_rollback(project_root)?;
    }
    run_git_output(project_root, &["rev-parse", "--verify", &format!("{}^0", entry_hash)])
        .map_err(|_| anyhow::anyhow!("记录不存在: {}", entry_hash))?;
    let point = run_git_output(project_root, &["rev-parse", "--verify", &format!("{}^", entry_hash)])
        .map(|s| s.trim().to_string())
        .map_err(|_| anyhow::anyhow!("第一条记录无法回退，没有更早的状态"))?;
    let head = get_current_commit_hash(project_root)?;

    let dirty = run_git_output(project_root, &["status", "--porcelain"])?
        .lines()
        .any(|l| !l.trim().is_empty());
    if dirty {
        run_git(project_root, &["stash", "push", "-u", "-m", ROLLBACK_STASH_PREFIX])?;
    }

    run_git(
        project_root,
        &["restore", "--source", &point, "--staged", "--worktree", "--", "."],
    )?;
    run_git(project_root, &["update-ref", ROLLBACK_REF, &head])?;
    run_git(project_root, &["update-ref", ROLLBACK_POINT_REF, &point])?;
    eprintln!("[debug] rollback → ok (point={})", point);
    Ok(())
}

pub struct RollbackState {
    pub rollback_point: Option<String>,
    pub head_matches: bool,
}

pub fn get_rollback_point_hash(project_root: &Path) -> Result<Option<String>> {
    Ok(get_rollback_state(project_root)?.rollback_point)
}

pub fn reset_soft_to(project_root: &Path, target: &str) -> Result<()> {
    eprintln!("[debug] reset_soft_to: root={:?}, target={}", project_root, target);
    run_git(project_root, &["reset", "--soft", target])
}

pub fn get_rollback_state(project_root: &Path) -> Result<RollbackState> {
    let point = run_git_output_quiet(project_root, &["rev-parse", "--verify", ROLLBACK_POINT_REF])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(point) = point else {
        return Ok(RollbackState { rollback_point: None, head_matches: false });
    };
    let saved_head = run_git_output_quiet(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?;
    let head = get_current_commit_hash(project_root)?;
    Ok(RollbackState {
        rollback_point: Some(point),
        head_matches: head == saved_head.trim(),
    })
}

fn rollback_stash_files(project_root: &Path, point: &str, stash_ref: &str) -> Vec<String> {
    run_git_output(project_root, &["-c", "core.quotepath=false", "diff", "--name-only", point, stash_ref])
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn rollback_local_files(project_root: &Path, point: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for args in [
        &["-c", "core.quotepath=false", "diff", "--name-only"][..],
        &["-c", "core.quotepath=false", "diff", "--cached", "--name-only", point][..],
    ] {
        if let Ok(out) = run_git_output(project_root, args) {
            files.extend(out.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()));
        }
    }
    if let Ok(untracked) = run_git_output(project_root, &["-c", "core.quotepath=false", "ls-files", "--others", "--exclude-standard"]) {
        files.extend(untracked.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()));
    }
    files.sort();
    files.dedup();
    files
}

fn run_git_stdin(path: &Path, args: &[&str], input: &str) -> Result<()> {
    use std::io::Write;
    let mut child = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn git {:?} at {:?}", args, path))?;
    let mut stdin = child.stdin.take().context("failed to open git stdin")?;
    stdin.write_all(input.as_bytes()).with_context(|| format!("failed to write to git {:?}", args))?;
    drop(stdin);
    let output = child.wait_with_output().with_context(|| format!("failed to wait for git {:?}", args))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[debug] git {:?} failed at {:?}: {}", args, path, stderr.trim());
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(())
}

fn run_git_output_quiet(path: &Path, args: &[&str]) -> Result<String> {
    let output = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("failed to execute git {:?} at {:?}", args, path))?;
    if !output.status.success() {
        anyhow::bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn stage_unmerged(project_root: &Path, path: &str, point: &str, stash_ref: &str) -> Result<()> {
    let zero = "0000000000000000000000000000000000000000";
    let mut info = format!("0 {zero}\t{path}\n");
    if let Ok(b) = run_git_output(project_root, &["rev-parse", &format!("{}:{}", point, path)]) {
        info.push_str(&format!("100644 {} 1\t{}\n", b.trim(), path));
    }
    if let Ok(o) = run_git_output(project_root, &["hash-object", "-w", path]) {
        info.push_str(&format!("100644 {} 2\t{}\n", o.trim(), path));
    }
    if let Ok(t) = run_git_output(project_root, &["rev-parse", &format!("{}:{}", stash_ref, path)]) {
        info.push_str(&format!("100644 {} 3\t{}\n", t.trim(), path));
    }
    run_git_stdin(project_root, &["update-index", "--index-info"], &info)
}

pub fn restore_rollback(project_root: &Path) -> Result<()> {
    eprintln!("[debug] restore_rollback: root={:?}", project_root);
    let saved_head = run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?;
    let point = run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_POINT_REF])?;
    let skip_paths = snapshot_skip_worktree(project_root);
    let Some(stash_ref) = find_rollback_stash(project_root)? else {
        // 无 stash：回退前工作区干净，回退期间改动自动合并带回（base=回退点, theirs=saved_head）
        let local_files = rollback_local_files(project_root, point.trim());
        if local_files.is_empty() {
            run_git(
                project_root,
                &["restore", "--source", saved_head.trim(), "--staged", "--worktree", "--", "."],
            )?;
            reapply_skip_worktree(project_root, &skip_paths)?;
            untrack_ignored_files(project_root)?;
            clear_rollback_refs(project_root)?;
            eprintln!("[debug] restore_rollback → ok (no stash, clean)");
            return Ok(());
        }
        // 被回退记录修改的文件（回退前状态相对回退点的差异）
        let theirs_files: Vec<String> = run_git_output(
            project_root,
            &["-c", "core.quotepath=false", "diff", "--name-only", point.trim(), saved_head.trim()],
        )?
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
        let conflicts: Vec<String> = theirs_files
            .iter()
            .filter(|f| local_files.contains(f))
            .filter(|f| {
                let theirs_blob = run_git_output(project_root, &["rev-parse", &format!("{}:{}", saved_head.trim(), f)]);
                let local_blob = run_git_output(project_root, &["hash-object", f]);
                match (&theirs_blob, &local_blob) {
                    (Ok(s), Ok(l)) => s.trim() != l.trim(),
                    (Err(_), Err(_)) => false,
                    _ => true,
                }
            })
            .cloned()
            .collect();
        if !conflicts.is_empty() {
            eprintln!("[debug] restore_rollback: conflicts {:?}", conflicts);
            for path in &conflicts {
                stage_unmerged(project_root, path, point.trim(), saved_head.trim())?;
            }
            anyhow::bail!("回退恢复冲突：{} 个文件需要二选一（保留当前版本或恢复最新版本）", conflicts.len());
        }
        // 仅将被回退记录修改的文件还原为回退前状态，回退期间只改的文件保留
        if !theirs_files.is_empty() {
            let mut args = vec!["restore", "--source", saved_head.trim(), "--staged", "--worktree", "--"];
            args.extend(theirs_files.iter().map(|s| s.as_str()));
            run_git(project_root, &args)?;
        }
        reapply_skip_worktree(project_root, &skip_paths)?;
        untrack_ignored_files(project_root)?;
        clear_rollback_refs(project_root)?;
        eprintln!("[debug] restore_rollback → ok (no stash, merged)");
        return Ok(());
    };

    let stash_files = rollback_stash_files(project_root, point.trim(), &stash_ref);
    let local_files = rollback_local_files(project_root, point.trim());
    let conflicts: Vec<String> = stash_files
        .iter()
        .filter(|f| local_files.contains(f))
        .filter(|f| {
            let stash_blob = run_git_output(project_root, &["rev-parse", &format!("{}:{}", stash_ref, f)]);
            let local_blob = run_git_output(project_root, &["hash-object", f]);
            match (&stash_blob, &local_blob) {
                (Ok(s), Ok(l)) => s.trim() != l.trim(),
                (Err(_), Err(_)) => false,
                _ => true,
            }
        })
        .cloned()
        .collect();
    if !conflicts.is_empty() {
        eprintln!("[debug] restore_rollback: conflicts {:?}", conflicts);
        for path in &conflicts {
            stage_unmerged(project_root, path, point.trim(), &stash_ref)?;
        }
        anyhow::bail!("回退恢复冲突：{} 个文件需要二选一（保留当前版本或恢复最新版本）", conflicts.len());
    }

    if !stash_files.is_empty() {
        let restore_targets: Vec<&String> = stash_files
            .iter()
            .filter(|f| run_git_output(project_root, &["rev-parse", &format!("{}:{}", stash_ref, f)]).is_ok())
            .collect();
        if !restore_targets.is_empty() {
            let mut args = vec!["restore", "--source", stash_ref.as_str(), "--staged", "--worktree", "--"];
            args.extend(restore_targets.iter().map(|s| s.as_str()));
            run_git(project_root, &args)?;
        }
    }
    let untracked_tree = format!("{}^{{3}}", stash_ref);
    if let Ok(untracked) = run_git_output(project_root, &["-c", "core.quotepath=false", "ls-tree", "-r", "--name-only", &untracked_tree]) {
        for p in untracked.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            if !project_root.join(&p).exists() {
                run_git(project_root, &["restore", "--source", &untracked_tree, "--staged", "--worktree", "--", &p])?;
            }
        }
    }
    reapply_skip_worktree(project_root, &skip_paths)?;
    untrack_ignored_files(project_root)?;
    run_git(project_root, &["stash", "drop", &stash_ref])?;
    clear_rollback_refs(project_root)?;
    eprintln!("[debug] restore_rollback → ok");
    Ok(())
}

pub fn get_rollback_conflicts(project_root: &Path) -> Result<Vec<String>> {
    let out = run_git_output(project_root, &["-c", "core.quotepath=false", "diff", "--name-only", "--diff-filter=U"])?;
    Ok(out.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}

pub fn resolve_rollback_conflicts(project_root: &Path, choices: &[(String, bool)]) -> Result<()> {
    eprintln!("[debug] resolve_rollback_conflicts: root={:?}, {} files", project_root, choices.len());
    let all = get_rollback_conflicts(project_root)?;
    for (path, _) in choices {
        if !all.contains(path) {
            anyhow::bail!("包含未冲突的文件: {}", path);
        }
    }
    let uncovered: Vec<&String> = all
        .iter()
        .filter(|p| !choices.iter().any(|(c, _)| c == *p))
        .collect();
    if !uncovered.is_empty() {
        anyhow::bail!("冲突文件列表不完整，未解决全部冲突文件：{:?}", uncovered);
    }
    // "回退前状态"的内容源：有 stash 时 = stash 树（回退前工作区），无 stash 时 = 回退前 HEAD
    let theirs: String = match find_rollback_stash(project_root)? {
        Some(s) => s,
        None => run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?.trim().to_string(),
    };
    let skip_paths = snapshot_skip_worktree(project_root);
    for (path, use_latest) in choices {
        if *use_latest {
            if run_git_output(project_root, &["rev-parse", &format!("{}:{}", theirs, path)]).is_ok() {
                // 用 git restore 写回（会跑 smudge filter，LFS 文件得到真实内容；
                // 直接用 git show 写文件不过 filter，LFS 文件会变成指针文本）
                run_git(project_root, &["restore", "--source", theirs.as_str(), "--worktree", "--", path])?;
            } else if project_root.join(path).is_file() {
                std::fs::remove_file(project_root.join(path))?;
            }
        }
        run_git(project_root, &["add", path])?;
    }
    reapply_skip_worktree(project_root, &skip_paths)?;
    untrack_ignored_files(project_root)?;
    if let Some(s) = find_rollback_stash(project_root)? {
        run_git(project_root, &["stash", "drop", &s])?;
    }
    clear_rollback_refs(project_root)?;
    eprintln!("[debug] resolve_rollback_conflicts → ok");
    Ok(())
}

pub fn clear_rollback(project_root: &Path) -> Result<()> {
    if let Some(s) = find_rollback_stash(project_root)? {
        let _ = run_git(project_root, &["stash", "drop", &s]);
    }
    clear_rollback_refs(project_root)
}

pub fn keep_rollback_file(project_root: &Path, path: &str) -> Result<()> {
    eprintln!("[debug] keep_rollback_file: root={:?}, path={}", project_root, path);
    let st = get_rollback_state(project_root)?;
    st.rollback_point
        .ok_or_else(|| anyhow::anyhow!("不在回退状态，无法保留文件"))?;
    let head = get_current_commit_hash(project_root)?;
    // 仅处理"回退前 HEAD 有、回退点没有"的删除差异文件
    run_git_output(project_root, &["rev-parse", "--verify", &format!("{}:{}", head, path)])
        .map_err(|_| anyhow::anyhow!("文件在回退前状态不存在，无需保留"))?;
    let in_index = run_git_output(project_root, &["ls-files", "--error-unmatch", path]).is_ok();
    if in_index {
        anyhow::bail!("文件已在项目中，无需保留");
    }
    // 从回退前 HEAD 恢复文件（index + 工作区），D 差异消失
    run_git(
        project_root,
        &["restore", "--source", &head, "--staged", "--worktree", "--", path],
    )?;
    eprintln!("[debug] keep_rollback_file → ok");
    Ok(())
}

fn get_current_branch(project_root: &Path) -> Result<String> {
    let branch = run_git_output(project_root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(branch.trim().to_string())
}

pub fn tracked_files(project_root: &Path) -> Result<Vec<String>> {
    let output = run_git_output(project_root, &[
        "-c", "core.quotepath=false",
        "ls-files",
    ])?;
    Ok(output.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect())
}

fn run_git_stdin_output(path: &Path, args: &[&str], input: &str) -> Result<String> {
    use std::io::Write;
    let mut child = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn git {:?} at {:?}", args, path))?;
    let mut stdin = child.stdin.take().context("failed to open git stdin")?;
    stdin.write_all(input.as_bytes()).with_context(|| format!("failed to write to git {:?}", args))?;
    drop(stdin);
    let output = child.wait_with_output().with_context(|| format!("failed to wait for git {:?}", args))?;
    if !output.status.success() {
        anyhow::bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 返回传入列表中命中 .gitignore 规则的路径（对已跟踪文件也返回匹配）
pub fn check_ignore(project_root: &Path, files: &[String]) -> Result<Vec<String>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    use std::io::Write;
    let input = files.join("\n") + "\n";
    let mut child = crate::process_util::hide_console(Command::new("git"))
        .args(["check-ignore", "-v", "--no-index", "--stdin"])
        .current_dir(project_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn git check-ignore at {:?}", project_root))?;
    let mut stdin = child.stdin.take().context("failed to open git stdin")?;
    stdin.write_all(input.as_bytes())?;
    drop(stdin);
    let output = child.wait_with_output().with_context(|| format!("failed to wait for git check-ignore at {:?}", project_root))?;
    // exit 0 = 有匹配，exit 1 = 无匹配（git check-ignore 的语义，非错误）
    if output.status.code() == Some(128) {
        anyhow::bail!("git check-ignore failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    // -v 输出格式: "<source>:<linenum>:<pattern>\t<pathname>"，取 tab 后路径
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(_, p)| p.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect())
}

/// 取消 gitignore 规则匹配的已跟踪文件的跟踪（隐藏 + 不参与提交，文件保留磁盘）
pub fn untrack_ignored_files(project_root: &Path) -> Result<()> {
    eprintln!("[debug] untrack_ignored_files: root={:?}", project_root);
    let tracked = tracked_files(project_root)?;
    if tracked.is_empty() {
        return Ok(());
    }
    let matched = check_ignore(project_root, &tracked)?;
    if matched.is_empty() {
        return Ok(());
    }
    for f in &matched {
        eprintln!("[debug] untrack_ignored_files: rm --cached {}", f);
        run_git(project_root, &["rm", "--cached", "--", f])?;
    }
    eprintln!("[debug] untrack_ignored_files → {} files untracked", matched.len());
    Ok(())
}

pub fn is_lfs_available() -> bool {
    crate::process_util::hide_console(Command::new("git"))
        .args(["lfs", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn lfs_auto_configure(project_root: &Path) -> Result<()> {
    if !is_lfs_available() {
        return Ok(());
    }

    let gitattributes = project_root.join(".gitattributes");

    let mut content = String::new();
    if gitattributes.exists() {
        content = std::fs::read_to_string(&gitattributes)
            .with_context(|| format!("failed to read {:?}", gitattributes))?;
    }

    let lfs_rule = "*.blend filter=lfs diff=lfs merge=lfs -text";

    if !content.contains(lfs_rule) {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(lfs_rule);
        content.push('\n');
        std::fs::write(&gitattributes, &content)
            .with_context(|| format!("failed to write {:?}", gitattributes))?;

        run_git(project_root, &["add", ".gitattributes"])?;
    }

    Ok(())
}

pub fn ensure_git_user(project_root: &Path) -> Result<()> {
    eprintln!("[debug] ensure_git_user: root={:?}", project_root);
    let name_ok = run_git_output(project_root, &["config", "user.name"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !name_ok {
        run_git(project_root, &["config", "user.name", "Keinestamp User"])?;
    }
    let email_ok = run_git_output(project_root, &["config", "user.email"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !email_ok {
        run_git(project_root, &["config", "user.email", "user@keinestamp"])?;
    }
    Ok(())
}

pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

fn run_git(path: &Path, args: &[&str]) -> Result<()> {
    let output = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("failed to execute git {:?} at {:?}", args, path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[debug] git {:?} failed at {:?}: {}", args, path, stderr.trim());
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(())
}

fn run_git_output(path: &Path, args: &[&str]) -> Result<String> {
    let output = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("failed to execute git {:?} at {:?}", args, path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[debug] git {:?} failed at {:?}: {}", args, path, stderr.trim());
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_git_project_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::process_util::hide_console(Command::new("git"))
            .args(["init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        crate::process_util::hide_console(Command::new("git"))
            .args(["config", "user.email", "test@test"])
            .current_dir(&dir)
            .output()
            .unwrap();
        crate::process_util::hide_console(Command::new("git"))
            .args(["config", "user.name", "test"])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn test_add_and_commit() -> Result<()> {
        let dir = tmp_repo("add_and_commit");
        std::fs::write(dir.join("test.txt"), b"hello")?;

        let hash = add_and_commit(&dir, "initial commit")?;
        assert_eq!(hash.len(), 40);

        let log_output = log(&dir, 1)?;
        assert!(log_output.contains(&hash));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_add_and_commit_files_commits_only_selected() -> Result<()> {
        let dir = tmp_repo("add_and_commit_files");
        std::fs::write(dir.join("a.txt"), b"a")?;
        std::fs::write(dir.join("b.txt"), b"b")?;
        add_and_commit(&dir, "base")?;

        std::fs::write(dir.join("a.txt"), b"a2")?;
        std::fs::write(dir.join("b.txt"), b"b2")?;

        let hash = add_and_commit_files(&dir, &["a.txt".to_string()], "only a")?;
        assert_eq!(hash.len(), 40);

        let stat = run_git_output(&dir, &["show", "--stat", "--name-only", "--format=", &hash])?;
        assert!(stat.contains("a.txt"));
        assert!(!stat.contains("b.txt"), "b.txt should not be in the first commit");

        let s = status(&dir)?;
        assert!(s.contains("b.txt"), "b.txt should still be uncommitted");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_add_and_commit_files_empty_fails() -> Result<()> {
        let dir = tmp_repo("add_and_commit_files_empty");
        std::fs::write(dir.join("a.txt"), b"a")?;
        add_and_commit(&dir, "base")?;
        assert!(add_and_commit_files(&dir, &[], "x").is_err());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_status_clean() -> Result<()> {
        let dir = tmp_repo("status_clean");
        std::fs::write(dir.join("test.txt"), b"hello")?;
        add_and_commit(&dir, "initial")?;

        let s = status(&dir)?;
        assert!(s.trim().is_empty());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_status_lists_untracked_files_inside_folder() -> Result<()> {
        let dir = tmp_repo("status_uall");
        std::fs::create_dir_all(dir.join("sub"))?;
        std::fs::write(dir.join("sub/a.txt"), b"a")?;
        std::fs::write(dir.join("sub/b.txt"), b"b")?;

        let s = status(&dir)?;
        assert!(s.contains("sub/a.txt"), "should list file a: {}", s);
        assert!(s.contains("sub/b.txt"), "should list file b: {}", s);
        assert!(
            !s.lines().any(|l| l.trim() == "?? sub/"),
            "should not collapse the folder: {}",
            s
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_status_dirty() -> Result<()> {
        let dir = tmp_repo("status_dirty");
        std::fs::write(dir.join("test.txt"), b"hello")?;
        add_and_commit(&dir, "initial")?;

        std::fs::write(dir.join("test.txt"), b"modified")?;
        let s = status(&dir)?;
        assert!(s.contains("M test.txt"));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_roundtrip() -> Result<()> {
        let dir = tmp_repo("rollback_roundtrip");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        std::fs::write(dir.join("b.txt"), b"new")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        let st = get_rollback_state(&dir)?;
        assert_eq!(st.rollback_point.as_deref(), Some(first.as_str()));
        assert!(st.head_matches);

        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_conflict_resolve() -> Result<()> {
        let dir = tmp_repo("rollback_conflict");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        std::fs::write(dir.join("a.txt"), b"v1.5")?;
        assert!(restore_rollback(&dir).is_err(), "两边都改过应冲突");
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "a.txt"));

        resolve_rollback_conflicts(&dir, &[("a.txt".to_string(), true)])?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_roundtrip_chinese() -> Result<()> {
        let dir = tmp_repo("rollback_roundtrip_chinese");
        std::fs::write(dir.join("测试文件.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("测试文件.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("测试文件.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("测试文件.txt"))?, b"v1");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_some(), "应处于回退状态");

        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("测试文件.txt"))?, b"v3");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_conflict_resolve_chinese() -> Result<()> {
        let dir = tmp_repo("rollback_conflict_chinese");
        std::fs::write(dir.join("测试文件.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("测试文件.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("测试文件.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("测试文件.txt"))?, b"v1");

        std::fs::write(dir.join("测试文件.txt"), b"v1.5")?;
        assert!(restore_rollback(&dir).is_err(), "两边都改过应冲突");
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "测试文件.txt"), "conflicts: {:?}", conflicts);

        resolve_rollback_conflicts(&dir, &[("测试文件.txt".to_string(), true)])?;
        assert_eq!(std::fs::read(dir.join("测试文件.txt"))?, b"v3");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_resolve_rollback_conflicts_incomplete_choices() -> Result<()> {
        let dir = tmp_repo("rollback_conflict_incomplete");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        std::fs::write(dir.join("b.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        std::fs::write(dir.join("b.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        std::fs::write(dir.join("b.txt"), b"v3")?;
        rollback(&dir, &second)?;

        std::fs::write(dir.join("a.txt"), b"v1.5")?;
        std::fs::write(dir.join("b.txt"), b"v1.5")?;
        assert!(restore_rollback(&dir).is_err(), "两边都改过应冲突");
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.contains(&"a.txt".to_string()));
        assert!(conflicts.contains(&"b.txt".to_string()));

        let err = resolve_rollback_conflicts(&dir, &[("a.txt".to_string(), true)])
            .expect_err("choices 不完整应报错");
        assert!(err.to_string().contains("冲突文件列表不完整"), "err: {}", err);
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_some(), "未全部解决时不应清理 refs");
        assert!(find_rollback_stash(&dir)?.is_some(), "未全部解决时不应 drop stash");

        resolve_rollback_conflicts(
            &dir,
            &[("a.txt".to_string(), true), ("b.txt".to_string(), false)],
        )?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3");
        assert_eq!(std::fs::read(dir.join("b.txt"))?, b"v1.5");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_no_stash_restore_conflict_resolvable() -> Result<()> {
        let dir = tmp_repo("rollback_no_stash_guard");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        assert!(find_rollback_stash(&dir)?.is_none(), "干净工作区回退不应产生 stash");

        std::fs::write(dir.join("a.txt"), b"user-change")?;
        let err = restore_rollback(&dir).expect_err("回退期间改动与回退记录重叠应冲突");
        assert!(err.to_string().contains("冲突"), "err: {}", err);
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"user-change", "冲突时工作区改动保留");
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "a.txt"), "conflicts: {:?}", conflicts);

        resolve_rollback_conflicts(&dir, &[("a.txt".to_string(), false)])?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"user-change", "保留回退期间修改");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none(), "解决后回退状态应清除");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_no_stash_restore_merges_non_conflict() -> Result<()> {
        let dir = tmp_repo("rollback_no_stash_merge");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        std::fs::write(dir.join("b.txt"), b"b1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        assert!(find_rollback_stash(&dir)?.is_none());

        std::fs::write(dir.join("b.txt"), b"b2")?;
        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2", "被回退记录的文件应还原为回退前状态");
        assert_eq!(std::fs::read(dir.join("b.txt"))?, b"b2", "回退期间只改的文件应保留改动");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_no_stash_restore_protects_staged_changes() -> Result<()> {
        let dir = tmp_repo("rollback_no_stash_staged_guard");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        assert!(find_rollback_stash(&dir)?.is_none(), "干净工作区回退不应产生 stash");

        std::fs::write(dir.join("a.txt"), b"user-change")?;
        run_git(&dir, &["add", "a.txt"])?;
        let err = restore_rollback(&dir).expect_err("回退期间已暂存改动与回退记录重叠应冲突");
        assert!(err.to_string().contains("冲突"), "err: {}", err);
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"user-change", "暂存改动必须原样保留");
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "a.txt"), "conflicts: {:?}", conflicts);

        resolve_rollback_conflicts(&dir, &[("a.txt".to_string(), false)])?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"user-change");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_staged_change_conflicts() -> Result<()> {
        let dir = tmp_repo("rollback_staged_conflict");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        std::fs::write(dir.join("b.txt"), b"b1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        std::fs::write(dir.join("b.txt"), b"b2")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        assert_eq!(std::fs::read(dir.join("b.txt"))?, b"b1");
        assert!(find_rollback_stash(&dir)?.is_some(), "脏工作区回退应产生 stash");

        std::fs::write(dir.join("b.txt"), b"b3")?;
        run_git(&dir, &["add", "b.txt"])?;
        let err = restore_rollback(&dir).expect_err("暂存改动与回退两侧都不同时应冲突");
        assert!(err.to_string().contains("冲突"), "err: {}", err);
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "b.txt"), "conflicts: {:?}", conflicts);

        resolve_rollback_conflicts(&dir, &[("b.txt".to_string(), true)])?;
        assert_eq!(std::fs::read(dir.join("b.txt"))?, b"b2", "latest 应恢复 stash 内容");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_conflict_resolve_latest_deletes_absent_in_stash() -> Result<()> {
        let dir = tmp_repo("rollback_resolve_deleted_stash");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::remove_file(dir.join("a.txt"))?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");
        assert!(find_rollback_stash(&dir)?.is_some(), "删除文件的工作区回退应产生 stash");

        std::fs::write(dir.join("a.txt"), b"user-a")?;
        let err = restore_rollback(&dir).expect_err("stash 删除了文件而用户重建时应冲突");
        assert!(err.to_string().contains("冲突"), "err: {}", err);
        let conflicts = get_rollback_conflicts(&dir)?;
        assert!(conflicts.iter().any(|c| c == "a.txt"), "conflicts: {:?}", conflicts);

        resolve_rollback_conflicts(&dir, &[("a.txt".to_string(), true)])?;
        assert!(!dir.join("a.txt").exists(), "latest 应保留 stash 的删除");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_find_rollback_stash_exact_match() -> Result<()> {
        let dir = tmp_repo("rollback_stash_exact");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        std::fs::write(dir.join("a.txt"), b"user")?;
        run_git(&dir, &["stash", "push", "-m", "some keinestamp-rollback mention"])?;

        let found = find_rollback_stash(&dir)?.expect("应找到回退 stash");
        assert_eq!(found, "stash@{1}", "必须精确匹配回退 stash，不能误认仅含子串的用户 stash");

        clear_rollback(&dir)?;
        let out = run_git_output(&dir, &["stash", "list"])?;
        assert_eq!(out.trim().lines().count(), 1, "应只剩用户 stash: {}", out);
        assert!(out.contains("some keinestamp-rollback mention"), "用户 stash 应保留: {}", out);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_skip_worktree_preserved_after_restore() -> Result<()> {
        let dir = tmp_repo("rollback_skip_worktree_preserved");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        run_git(&dir, &["update-index", "--skip-worktree", "a.txt"])?;
        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3");
        let v = run_git_output(&dir, &["ls-files", "-v"])?;
        assert!(v.lines().any(|l| l.starts_with('S') && l.trim_end() == "S a.txt"), "恢复后 skip-worktree 标志应保留: {}", v);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_no_stash_gitignore_only_restore() -> Result<()> {
        let dir = tmp_repo("rollback_no_stash_gitignore");
        std::fs::write(dir.join(".gitignore"), b"out/\n")?;
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        let mut gi = std::fs::read_to_string(dir.join(".gitignore"))?;
        gi.push_str("cache/\n");
        std::fs::write(dir.join(".gitignore"), &gi)?;
        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v2");
        let gi_after = std::fs::read_to_string(dir.join(".gitignore"))?;
        assert!(gi_after.contains("cache/"), "恢复后 .gitignore 的忽略规则应保留: {}", gi_after);
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_invalid_hash_leaves_no_stash() -> Result<()> {
        let dir = tmp_repo("rollback_bad_hash");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        let err = rollback(&dir, "0123456789abcdef0123456789abcdef01234567")
            .expect_err("非法记录 hash 应报错");
        assert!(err.to_string().contains("记录不存在"), "err: {}", err);
        let stash_out = run_git_output(&dir, &["stash", "list"])?;
        assert!(stash_out.trim().is_empty(), "不应留下孤儿 stash: {}", stash_out);
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none(), "不应留下回退状态");
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3", "工作区改动应保留");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_first_entry_rejected() -> Result<()> {
        let dir = tmp_repo("rollback_first_entry");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;
        let first = get_current_commit_hash(&dir)?;

        let err = rollback(&dir, &first).expect_err("第一条记录没有更早状态，应报错");
        assert!(err.to_string().contains("第一条记录无法回退"), "err: {}", err);
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none(), "不应留下回退状态");
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1", "工作区不应被改动");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_while_in_rollback_auto_restores() -> Result<()> {
        let dir = tmp_repo("rollback_nested");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1", "再次回退应自动恢复后回到目标状态");
        let st = get_rollback_state(&dir)?;
        assert_eq!(st.rollback_point.as_deref(), Some(first.as_str()), "回退点应为 first");
        assert!(st.head_matches);
        let stash_out = run_git_output(&dir, &["stash", "list"])?;
        assert_eq!(stash_out.trim().lines().count(), 1, "自动恢复应 drop 旧 stash，回退应新建一个: {}", stash_out);

        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v3", "恢复应回到回退前状态");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_identical_edits_no_conflict() -> Result<()> {
        let dir = tmp_repo("rollback_identical");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"same")?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        std::fs::write(dir.join("a.txt"), b"same")?;
        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"same", "两边相同内容不应视为冲突");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_rollback_both_deleted_no_conflict() -> Result<()> {
        let dir = tmp_repo("rollback_both_deleted");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        std::fs::remove_file(dir.join("a.txt"))?;
        rollback(&dir, &second)?;
        assert_eq!(std::fs::read(dir.join("a.txt"))?, b"v1");

        std::fs::remove_file(dir.join("a.txt"))?;
        restore_rollback(&dir)?;
        assert!(!dir.join("a.txt").exists(), "两边都删除应保持删除");
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_keep_rollback_file() -> Result<()> {
        let dir = tmp_repo("rollback_keep_file");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("b.txt"), b"b1")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert!(!dir.join("b.txt").exists(), "回退后 b.txt 应从工作区移除");
        assert!(run_git_output(&dir, &["ls-files", "--error-unmatch", "b.txt"]).is_err(), "回退后 b.txt 不在 index");

        keep_rollback_file(&dir, "b.txt")?;
        assert_eq!(std::fs::read(dir.join("b.txt"))?, b"b1", "保留后文件应回到工作区");
        assert!(run_git_output(&dir, &["ls-files", "--error-unmatch", "b.txt"]).is_ok(), "保留后 b.txt 应在 index");
        let status = status(&dir)?;
        assert!(!status.contains("D b.txt"), "保留后不应再显示删除差异: {}", status);

        let err = keep_rollback_file(&dir, "a.txt").expect_err("已在项目中的文件应报错");
        assert!(err.to_string().contains("无需保留"), "err: {}", err);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_keep_rollback_file_outside_rollback() -> Result<()> {
        let dir = tmp_repo("rollback_keep_no_state");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        let err = keep_rollback_file(&dir, "a.txt").expect_err("非回退状态应报错");
        assert!(err.to_string().contains("不在回退状态"), "err: {}", err);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_restore_untracks_ignored_files() -> Result<()> {
        let dir = tmp_repo("rollback_untrack_ignored");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("x.txt"), b"x1")?;
        add_and_commit(&dir, "second")?;
        let second = get_current_commit_hash(&dir)?;

        rollback(&dir, &second)?;
        assert!(!dir.join("x.txt").exists(), "回退后 x.txt 应从工作区移除");

        let mut gi = std::fs::read_to_string(dir.join(".gitignore")).unwrap_or_default();
        gi.push_str("x.txt\n");
        std::fs::write(dir.join(".gitignore"), &gi)?;

        restore_rollback(&dir)?;
        assert_eq!(std::fs::read(dir.join("x.txt"))?, b"x1", "恢复后 x.txt 文件应保留在磁盘");
        let tracked = tracked_files(&dir)?;
        assert!(!tracked.iter().any(|t| t == "x.txt"), "忽略的文件应取消跟踪: {:?}", tracked);
        // 前端隐藏依据：check-ignore 匹配（status 的 D 差异由 UI 层隐藏）
        let matched = check_ignore(&dir, &["x.txt".to_string()])?;
        assert_eq!(matched, vec!["x.txt".to_string()], "规则应匹配 x.txt（前端据此隐藏）");
        // 记录后 x.txt 不参与提交
        add_and_commit(&dir, "after-restore")?;
        let tracked_after = tracked_files(&dir)?;
        assert!(!tracked_after.iter().any(|t| t == "x.txt"), "记录后 x.txt 不应被提交: {:?}", tracked_after);
        let st = get_rollback_state(&dir)?;
        assert!(st.rollback_point.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_check_ignore_matches_tracked() -> Result<()> {
        let dir = tmp_repo("check_ignore_tracked");
        std::fs::write(dir.join("a.txt"), b"v1")?;
        add_and_commit(&dir, "first")?;

        let matched = check_ignore(&dir, &["a.txt".to_string(), "b.txt".to_string()])?;
        assert!(matched.is_empty(), "无规则时不应匹配: {:?}", matched);

        let mut gi = std::fs::read_to_string(dir.join(".gitignore")).unwrap_or_default();
        gi.push_str("b.txt\n");
        std::fs::write(dir.join(".gitignore"), &gi)?;

        let matched = check_ignore(&dir, &["a.txt".to_string(), "b.txt".to_string()])?;
        assert_eq!(matched, vec!["b.txt".to_string()], "应匹配规则命中的路径: {:?}", matched);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_is_git_repo() {
        let dir = tmp_repo("is_git_repo");
        assert!(is_git_repo(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_is_lfs_available_detection() {
        let available = is_lfs_available();
        println!("Git LFS available: {}", available);
    }

    #[test]
    fn test_lfs_auto_configure() -> Result<()> {
        let dir = tmp_repo("lfs_configure");
        lfs_auto_configure(&dir)?;

        let attributes = dir.join(".gitattributes");
        if is_lfs_available() {
            assert!(attributes.exists());
            let content = std::fs::read_to_string(&attributes)?;
            assert!(content.contains("*.blend filter=lfs"));
        }

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
