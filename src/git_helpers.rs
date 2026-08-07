use std::path::Path;

pub fn get_changed_files(project_root: &Path, commit_hash: &str) -> anyhow::Result<Vec<String>> {
    let is_root = std::process::Command::new("git")
        .args(["rev-parse", &format!("{}^", commit_hash)])
        .current_dir(project_root)
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true);

    let mut args: Vec<&str> = vec![
        "-c", "core.quotepath=false",
        "diff-tree",
        "--no-commit-id",
        "--name-only",
        "-r",
    ];
    if is_root {
        args.push("--root");
    }
    args.push(commit_hash);

    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff-tree failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect())
}

pub fn get_changed_files_status(project_root: &Path, commit_hash: &str) -> anyhow::Result<Vec<String>> {
    let is_root = std::process::Command::new("git")
        .args(["rev-parse", &format!("{}^", commit_hash)])
        .current_dir(project_root)
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true);

    let mut args: Vec<&str> = vec![
        "-c", "core.quotepath=false",
        "diff-tree",
        "--no-commit-id",
        "--name-status",
        "-r",
    ];
    if is_root {
        args.push("--root");
    }
    args.push(commit_hash);

    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff-tree --name-status failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() { return None; }
            Some(line.chars().next().map(|c| c.to_string()).unwrap_or_default())
        })
        .collect())
}

pub fn has_blend_files(files: &[String]) -> bool {
    files.iter().any(|f| f.ends_with(".blend"))
}

pub fn get_git_author(project_root: &Path) -> String {
    std::process::Command::new("git")
        .args(["config", "user.name"])
        .current_dir(project_root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_blend_files() {
        assert!(has_blend_files(&["model.blend".to_string()]));
        assert!(has_blend_files(&["a.blend".to_string(), "b.txt".to_string()]));
        assert!(!has_blend_files(&["a.txt".to_string()]));
        assert!(!has_blend_files(&[]));
    }
}
