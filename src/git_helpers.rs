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

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

pub fn is_image_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
}

pub fn image_mime(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "image/png"
    }
}

pub fn image_to_data_uri(project_root: &Path, commit_hash: &str, file_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["-c", "core.quotepath=false", "show", &format!("{}:{}", commit_hash, file_path)])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git show failed for {}: {}", file_path, stderr.trim());
    }
    if output.stdout.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("image too large: {} bytes", output.stdout.len());
    }

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&output.stdout);
    Ok(format!("data:{};base64,{}", image_mime(file_path), b64))
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