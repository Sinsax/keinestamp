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
            if msg.contains(ROLLBACK_STASH_PREFIX) {
                Some(refname.to_string())
            } else {
                None
            }
        }))
}

fn run_git_bytes(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = crate::process_util::hide_console(Command::new("git"))
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("failed to execute git {:?} at {:?}", args, path))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(output.stdout)
}

fn clear_rollback_refs(project_root: &Path) -> Result<()> {
    let _ = run_git(project_root, &["update-ref", "-d", ROLLBACK_REF]);
    let _ = run_git(project_root, &["update-ref", "-d", ROLLBACK_POINT_REF]);
    Ok(())
}

pub fn rollback(project_root: &Path, entry_hash: &str) -> Result<()> {
    eprintln!("[debug] rollback: root={:?}, entry={}", project_root, entry_hash);
    let head = get_current_commit_hash(project_root)?;

    let dirty = run_git_output(project_root, &["status", "--porcelain"])?
        .lines()
        .any(|l| !l.trim().is_empty());
    if dirty {
        run_git(project_root, &["stash", "push", "-u", "-m", ROLLBACK_STASH_PREFIX])?;
    }

    run_git(
        project_root,
        &["restore", "--source", entry_hash, "--staged", "--worktree", "--", "."],
    )?;
    run_git(project_root, &["update-ref", ROLLBACK_REF, &head])?;
    run_git(project_root, &["update-ref", ROLLBACK_POINT_REF, entry_hash])?;
    eprintln!("[debug] rollback → ok");
    Ok(())
}

pub struct RollbackState {
    pub rollback_point: Option<String>,
    pub head_matches: bool,
}

pub fn get_rollback_state(project_root: &Path) -> Result<RollbackState> {
    let point = run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_POINT_REF])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(point) = point else {
        return Ok(RollbackState { rollback_point: None, head_matches: false });
    };
    let saved_head = run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_REF])?;
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

fn rollback_local_files(project_root: &Path) -> Vec<String> {
    let mut files: Vec<String> = run_git_output(project_root, &["-c", "core.quotepath=false", "diff", "--name-only"])
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if let Ok(untracked) = run_git_output(project_root, &["-c", "core.quotepath=false", "ls-files", "--others", "--exclude-standard"]) {
        files.extend(untracked.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()));
    }
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
    let Some(stash_ref) = find_rollback_stash(project_root)? else {
        run_git(
            project_root,
            &["restore", "--source", saved_head.trim(), "--staged", "--worktree", "--", "."],
        )?;
        clear_rollback_refs(project_root)?;
        eprintln!("[debug] restore_rollback → ok (no stash)");
        return Ok(());
    };

    let point = run_git_output(project_root, &["rev-parse", "--verify", ROLLBACK_POINT_REF])?;
    let stash_files = rollback_stash_files(project_root, point.trim(), &stash_ref);
    let local_files = rollback_local_files(project_root);
    let conflicts: Vec<String> = stash_files
        .iter()
        .filter(|f| local_files.contains(f))
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
        let mut args = vec!["restore", "--source", stash_ref.as_str(), "--staged", "--worktree", "--"];
        args.extend(stash_files.iter().map(|s| s.as_str()));
        run_git(project_root, &args)?;
    }
    let untracked_tree = format!("{}^{{3}}", stash_ref);
    if let Ok(untracked) = run_git_output(project_root, &["-c", "core.quotepath=false", "ls-tree", "-r", "--name-only", &untracked_tree]) {
        for p in untracked.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            if !project_root.join(&p).exists() {
                run_git(project_root, &["restore", "--source", &untracked_tree, "--staged", "--worktree", "--", &p])?;
            }
        }
    }
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
    let uncovered: Vec<&String> = all
        .iter()
        .filter(|p| !choices.iter().any(|(c, _)| c == *p))
        .collect();
    if !uncovered.is_empty() {
        anyhow::bail!("冲突文件列表不完整，未解决全部冲突文件：{:?}", uncovered);
    }
    let stash_ref = find_rollback_stash(project_root)?
        .ok_or_else(|| anyhow::anyhow!("没有找到回退暂存（stash）"))?;
    for (path, use_latest) in choices {
        if *use_latest {
            let content = run_git_bytes(project_root, &["show", &format!("{}:{}", stash_ref, path)])?;
            let full = project_root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full, &content)?;
        }
        run_git(project_root, &["add", path])?;
    }
    run_git(project_root, &["stash", "drop", &stash_ref])?;
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

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &first)?;
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
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        rollback(&dir, &first)?;
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
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("测试文件.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;

        std::fs::write(dir.join("测试文件.txt"), b"v3")?;
        rollback(&dir, &first)?;
        assert_eq!(std::fs::read(dir.join("测试文件.txt"))?, b"v1");

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
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("测试文件.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;

        std::fs::write(dir.join("测试文件.txt"), b"v3")?;
        rollback(&dir, &first)?;
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
        let first = get_current_commit_hash(&dir)?;

        std::fs::write(dir.join("a.txt"), b"v2")?;
        std::fs::write(dir.join("b.txt"), b"v2")?;
        add_and_commit(&dir, "second")?;

        std::fs::write(dir.join("a.txt"), b"v3")?;
        std::fs::write(dir.join("b.txt"), b"v3")?;
        rollback(&dir, &first)?;

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
