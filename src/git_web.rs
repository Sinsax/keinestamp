use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn web_commit_and_push(web_root: &Path, message: &str, push_enabled: bool) -> Result<()> {
    eprintln!("[debug] web_commit_and_push: root={:?}, msg={:?}, push={}", web_root, message, push_enabled);
    run_web_git(web_root, &["add", "."])?;
    run_web_git(web_root, &["commit", "-m", message])?;

    if push_enabled {
        let branch = get_web_branch(web_root)?;
        let result = Command::new("git")
            .args(["push", "-u", "origin", &branch])
            .current_dir(web_root)
            .output();

        match result {
            Ok(output) if output.status.success() => {
                eprintln!("Web repo pushed successfully");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Web repo push skipped (no remote or error): {}", stderr.trim());
            }
            Err(e) => {
                eprintln!("Web repo push skipped: {}", e);
            }
        }
    }

    Ok(())
}

pub fn is_web_repo(web_root: &Path) -> bool {
    web_root.join(".git").exists()
}

fn get_web_branch(web_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(web_root)
        .output()
        .context("failed to get web repo branch")?;

    if !output.status.success() {
        return Ok("main".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_web_git(path: &Path, args: &[&str]) -> Result<()> {
    eprintln!("[debug] web git {:?} at {:?}", args, path);
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| {
            format!(
                "failed to execute git web {:?} at {:?}",
                args, path
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git web {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_web_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("saki_git_web_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "web@test"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "web"])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn test_web_commit_and_push_no_remote() -> Result<()> {
        let dir = tmp_web_repo("no_remote");
        std::fs::write(dir.join("timeline.json"), b"{}")?;

        web_commit_and_push(&dir, "update timeline", false)?;

        let output = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("update timeline"));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_is_web_repo() {
        let dir = tmp_web_repo("is_web_repo");
        assert!(is_web_repo(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
