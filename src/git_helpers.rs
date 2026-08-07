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

    let bytes = if looks_like_lfs_pointer(&output.stdout) {
        let mut child = std::process::Command::new("git")
            .args(["lfs", "smudge"])
            .current_dir(project_root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        use std::io::Write;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to open lfs smudge stdin"))?
            .write_all(&output.stdout)?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("git lfs smudge failed for {}: {}", file_path, stderr.trim());
        }
        out.stdout
    } else {
        output.stdout
    };

    if bytes.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("image too large: {} bytes", bytes.len());
    }

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", image_mime(file_path), b64))
}

fn looks_like_lfs_pointer(bytes: &[u8]) -> bool {
    bytes.starts_with(b"version https://git-lfs.github.com/spec/v1")
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

    #[test]
    fn test_is_image_file_and_mime() {
        assert!(is_image_file("a.PNG"));
        assert!(is_image_file("b.jpg"));
        assert!(is_image_file("c.jpeg"));
        assert!(is_image_file("d.webp"));
        assert!(!is_image_file("e.blend"));
        assert_eq!(image_mime("x.jpg"), "image/jpeg");
        assert_eq!(image_mime("x.webp"), "image/webp");
        assert_eq!(image_mime("x.png"), "image/png");
    }

    #[test]
    fn test_image_to_data_uri_plain_and_lfs() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_img_lfs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;

        let git = |args: &[&str]| {
            let out = std::process::Command::new("git").args(args).current_dir(&dir).output()?;
            if !out.status.success() {
                anyhow::bail!("git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
            }
            Ok(())
        };
        git(&["init"])?;
        git(&["config", "user.email", "t@t"])?;
        git(&["config", "user.name", "t"])?;

        // plain file
        let plain: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4];
        std::fs::write(dir.join("plain.png"), &plain)?;
        git(&["add", "plain.png"])?;
        git(&["commit", "-m", "plain"])?;

        // LFS file (requires git-lfs)
        let lfs_ok = std::process::Command::new("git")
            .args(["lfs", "install", "--local"])
            .current_dir(&dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !lfs_ok {
            std::fs::remove_dir_all(&dir)?;
            eprintln!("git-lfs unavailable, skipping LFS case");
        } else {
            std::fs::write(dir.join(".gitattributes"), "*.png filter=lfs diff=lfs merge=lfs -text\n")?;
            let lfs_bytes: Vec<u8> = (0..200).map(|i| (i % 251) as u8).collect();
            std::fs::write(dir.join("big.png"), &lfs_bytes)?;
            git(&["add", ".gitattributes", "big.png"])?;
            git(&["commit", "-m", "lfs"])?;
        }

        let head = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()?;
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();

        let plain_uri = image_to_data_uri(&dir, &head, "plain.png")?;
        assert!(plain_uri.starts_with("data:image/png;base64,"));

        if lfs_ok {
            use base64::Engine;
            let lfs_uri = image_to_data_uri(&dir, &head, "big.png")?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(lfs_uri.strip_prefix("data:image/png;base64,").unwrap())
                .unwrap();
            assert_eq!(decoded.len(), 200, "LFS image should decode to full bytes");
        }

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}