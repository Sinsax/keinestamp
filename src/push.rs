use anyhow::Result;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::FileOptions;

pub fn deploy_to_server(
    web_root: &Path,
    server_url: &str,
    deploy_token: Option<&str>,
    access_token: Option<&str>,
    project_name: &str,
    project_id: &str,
) -> Result<String> {
    if !web_root.exists() {
        anyhow::bail!("web root does not exist: {}", web_root.display());
    }

    let zip_bytes = create_zip(web_root).map_err(|e| {
        eprintln!("[debug] deploy_to_server: create_zip failed: {}", e);
        anyhow::anyhow!("打包网页文件失败: {}", e)
    })?;
    eprintln!("[debug] deploy_to_server: zip size={} bytes", zip_bytes.len());

    let sanitized_name = crate::serve::sanitize_project_name(project_name);
    let safe_project_name = if sanitized_name.is_empty() {
        let suffix = &project_id[..8.min(project_id.len())];
        format!("project{}", suffix)
    } else {
        sanitized_name
    };

    let upload_url = format!("{}/upload", server_url.trim_end_matches('/'));
    eprintln!("[debug] deploy_to_server: posting to {}", upload_url);

    let web_count = crate::git_web::get_web_commit_count(web_root);
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .build();
    let mut req = agent
        .post(&upload_url)
        .set("Content-Type", "application/zip")
        .set("X-Project-Name", &safe_project_name)
        .set("X-Project-Id", project_id)
        .set("X-Web-Count", &web_count.to_string());

    if let Some(token) = deploy_token {
        if !token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", token));
        }
    }

    if let Some(token) = access_token {
        if !token.is_empty() {
            req = req.set("X-Access-Token", token);
        }
    }
    let resp = req
        .send_bytes(&zip_bytes)
        .map_err(|e| {
            eprintln!("[debug] deploy_to_server: ureq error: {}", e);
            anyhow::anyhow!("连接服务器失败: {}", e)
        })?;

    let status = resp.status();
    let body = resp
        .into_string()
        .unwrap_or_else(|_| "unknown".to_string());

    if status == 200 {
        eprintln!("[debug] deploy_to_server: server returned 200 OK");
        let prefix = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("prefix")?.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        Ok(prefix)
    } else {
        eprintln!("[debug] deploy_to_server: server returned {}: {}", status, body);
        anyhow::bail!("server returned {}: {}", status, body)
    }
}

fn create_zip(dir: &Path) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));

    add_dir_to_zip(&mut zip, dir, dir, "")?;

    zip.finish()?;
    Ok(buffer)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::io::Cursor<&mut Vec<u8>>>,
    base: &Path,
    dir: &Path,
    prefix: &str,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == ".git" || name_str == "three" {
            continue;
        }

        let relative = if prefix.is_empty() {
            name_str.to_string()
        } else {
            format!("{}/{}", prefix, name_str)
        };

        if path.is_dir() {
            zip.add_directory::<&str, ()>(&relative, FileOptions::<()>::default())?;
            add_dir_to_zip(zip, base, &path, &relative)?;
        } else if path.is_file() {
            zip.start_file::<&str, ()>(&relative, FileOptions::<()>::default())?;
            let mut file = std::fs::File::open(&path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_name(project_name: &str, project_id: &str) -> String {
        let sanitized = crate::serve::sanitize_project_name(project_name);
        if sanitized.is_empty() {
            let suffix = &project_id[..8.min(project_id.len())];
            format!("project{}", suffix)
        } else {
            sanitized
        }
    }

    #[test]
    fn sanitize_keeps_ascii() {
        assert_eq!(safe_name("33_芙姆米_新1皮", "deadbeef1234"), "33__1");
    }

    #[test]
    fn sanitize_all_chinese_falls_back_to_project_id() {
        assert_eq!(safe_name("芙姆米", "deadbeef1234"), "projectdeadbeef");
    }

    #[test]
    fn sanitize_truncates_at_64() {
        let long = "a".repeat(80);
        assert_eq!(safe_name(&long, "deadbeef"), "a".repeat(64));
    }

    #[test]
    fn sanitize_empty_name_falls_back() {
        assert_eq!(safe_name("", "deadbeef"), "projectdeadbeef");
    }

    #[test]
    fn create_zip_includes_files_excludes_git_and_three() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("ks_push_zip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git"))?;
        std::fs::create_dir_all(dir.join("three"))?;
        std::fs::create_dir_all(dir.join("previews_glb"))?;
        std::fs::write(dir.join("index.html"), b"<html>")?;
        std::fs::write(dir.join(".git/config"), b"x")?;
        std::fs::write(dir.join("three/three.cjs"), b"x")?;
        std::fs::write(dir.join("previews_glb/a.glb"), b"glb")?;

        let bytes = create_zip(&dir)?;
        let cursor = std::io::Cursor::new(&bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "index.html"), "index.html should be in zip: {:?}", names);
        assert!(names.iter().any(|n| n == "previews_glb/a.glb"), "nested file should be in zip: {:?}", names);
        assert!(!names.iter().any(|n| n.contains(".git") || n.contains("three")), ".git/three must be excluded: {:?}", names);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn create_zip_missing_dir_fails() {
        let missing = std::env::temp_dir().join(format!("ks_push_missing_{}", std::process::id()));
        assert!(create_zip(&missing).is_err());
    }
}
