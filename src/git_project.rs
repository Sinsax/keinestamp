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

    run_git(project_root, &["commit", "-m", message])?;

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

pub fn revert(project_root: &Path) -> Result<()> {
    eprintln!("[debug] revert: root={:?}", project_root);
    let count = get_commit_count(project_root)?;
    if count < 2 {
        anyhow::bail!("no commits to revert");
    }
    run_git(project_root, &["reset", "--soft", "HEAD~1"])?;
    eprintln!("[debug] revert → ok");
    Ok(())
}

fn get_commit_count(project_root: &Path) -> Result<usize> {
    let output = run_git_output(project_root, &["rev-list", "--count", "HEAD"])?;
    Ok(output.trim().parse().unwrap_or(0))
}

pub fn status(project_root: &Path) -> Result<String> {
    let output = run_git_output(project_root, &[
        "-c", "core.quotepath=false",
        "status", "--porcelain",
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
    Command::new("git")
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
    let output = Command::new("git")
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
    let output = Command::new("git")
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
        Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
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
    fn test_revert() -> Result<()> {
        let dir = tmp_repo("revert");
        std::fs::write(dir.join("a.txt"), b"a")?;
        add_and_commit(&dir, "first")?;

        std::fs::write(dir.join("b.txt"), b"b")?;
        add_and_commit(&dir, "second")?;

        let log_before = log(&dir, 10)?;
        assert!(log_before.lines().count() >= 2);

        revert(&dir)?;

        let log_after = log(&dir, 10)?;
        let msg = git_project_log_message(&log_after);
        assert!(!msg.contains("second") || log_after.lines().count() < log_before.lines().count());

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

    fn git_project_log_message(log_output: &str) -> String {
        log_output
            .lines()
            .next()
            .map(|l| l.splitn(3, '|').nth(2).unwrap_or(""))
            .unwrap_or("")
            .to_string()
    }
}
