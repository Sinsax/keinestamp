use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::FileOptions;

/// 部署文件清单条目。客户端把完整期望文件树发给服务端，
/// 服务端据此做增量合并（只传输 changed 文件，删除多余文件）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployFile {
    pub path: String,
    pub size: u64,
    pub hash: String,
}

/// zip 内携带的清单文件名；服务端读取后不会写入部署目录。
pub const MANIFEST_ENTRY: &str = ".keinestamp-manifest.json";

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

    let sanitized_name = crate::serve::sanitize_project_name(project_name);
    let safe_project_name = if sanitized_name.is_empty() {
        let suffix = &project_id[..8.min(project_id.len())];
        format!("project{}", suffix)
    } else {
        sanitized_name
    };

    // web_root = <项目根>/.keinestamp/web → 项目根 = parent().parent()
    let project_root = web_root
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(web_root);
    let web_count = crate::lore_project::web_revision_count(project_root).unwrap_or(0);

    // 完整本地清单 + 服务端当前清单 → 只打包 changed 文件
    let manifest = collect_manifest(web_root)?;
    let remote_manifest =
        fetch_remote_manifest(server_url, &safe_project_name, deploy_token, access_token)
            .unwrap_or_default();
    let remote_map: HashMap<String, DeployFile> = remote_manifest
        .into_iter()
        .map(|f| (f.path.clone(), f))
        .collect();

    let mut changed: Vec<String> = Vec::new();
    for f in &manifest {
        let same = remote_map
            .get(&f.path)
            .map_or(false, |r| r.hash == f.hash && r.size == f.size);
        if !same {
            changed.push(f.path.clone());
        }
    }
    let local_paths: HashSet<&str> = manifest.iter().map(|f| f.path.as_str()).collect();
    let deleted: Vec<String> = remote_map
        .keys()
        .filter(|p| !local_paths.contains(p.as_str()))
        .cloned()
        .collect();

    let zip_bytes = create_incremental_zip(web_root, &manifest, &changed).map_err(|e| {
        eprintln!("[debug] deploy_to_server: create_zip failed: {}", e);
        anyhow::anyhow!("打包网页文件失败: {}", e)
    })?;
    eprintln!(
        "[debug] deploy_to_server: changed={} deleted={} zip_size={} bytes",
        changed.len(),
        deleted.len(),
        zip_bytes.len()
    );

    let upload_url = format!("{}/upload", server_url.trim_end_matches('/'));
    eprintln!("[debug] deploy_to_server: posting to {}", upload_url);

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .build();
    let mut req = agent
        .post(&upload_url)
        .set("Content-Type", "application/zip")
        .set("X-Project-Name", &safe_project_name)
        .set("X-Project-Id", project_id)
        .set("X-Web-Count", &web_count.to_string())
        .set("X-Incremental", "1");

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
    let resp = match req.send_bytes(&zip_bytes) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            eprintln!(
                "[debug] deploy_to_server: server error {}: {}",
                code, body
            );
            anyhow::bail!("服务器返回 {}: {}", code, body)
        }
        Err(e) => {
            eprintln!("[debug] deploy_to_server: ureq error: {}", e);
            anyhow::bail!("连接服务器失败: {}", e)
        }
    };

    let status = resp.status();
    let body = resp.into_string().unwrap_or_else(|_| "unknown".to_string());

    if status == 200 {
        eprintln!("[debug] deploy_to_server: server returned 200 OK");
        let prefix = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("prefix")?.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        Ok(prefix)
    } else {
        eprintln!(
            "[debug] deploy_to_server: server returned {}: {}",
            status, body
        );
        anyhow::bail!("server returned {}: {}", status, body)
    }
}

/// 读取服务端已部署项目的清单；拿不到就按空处理（首次部署/旧服务端 → 全量上传）。
fn fetch_remote_manifest(
    server_url: &str,
    project_name: &str,
    deploy_token: Option<&str>,
    access_token: Option<&str>,
) -> Option<Vec<DeployFile>> {
    let url = format!(
        "{}/{}/manifest",
        server_url.trim_end_matches('/'),
        project_name
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut req = agent.get(&url);
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
    let resp = match req.call() {
        Ok(r) => r,
        Err(_) => return None,
    };
    if resp.status() != 200 {
        return None;
    }
    let body = resp.into_string().ok()?;
    serde_json::from_str(&body).ok()
}

fn create_incremental_zip(
    dir: &Path,
    manifest: &[DeployFile],
    changed: &[String],
) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut zip = zip::ZipWriter::new(Cursor::new(&mut buffer));

    let manifest_json = serde_json::to_string(manifest)?;
    zip.start_file::<&str, ()>(MANIFEST_ENTRY, FileOptions::<()>::default())?;
    zip.write_all(manifest_json.as_bytes())?;

    let changed_set: HashSet<&str> = changed.iter().map(String::as_str).collect();
    for f in manifest {
        if changed_set.contains(f.path.as_str()) {
            zip.start_file::<&str, ()>(&f.path, FileOptions::<()>::default())?;
            let src = dir.join(&f.path);
            let mut file = std::fs::File::open(&src)?;
            // 流式写入，避免大预览文件整体载入内存
            std::io::copy(&mut file, &mut zip)?;
        }
    }

    zip.finish()?;
    Ok(buffer)
}

fn collect_manifest(dir: &Path) -> Result<Vec<DeployFile>> {
    let mut files = Vec::new();
    collect_files(dir, dir, "", &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn collect_files(base: &Path, dir: &Path, prefix: &str, out: &mut Vec<DeployFile>) -> Result<()> {
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
            collect_files(base, &path, &relative, out)?;
        } else if path.is_file() {
            let metadata = std::fs::metadata(&path)?;
            let hash = hash_file(&path)?;
            out.push(DeployFile {
                path: relative.replace('\\', "/"),
                size: metadata.len(),
                hash,
            });
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    let mut hash: u64 = 0xcbf29ce484222325;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("{:016x}", hash))
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

        let manifest = collect_manifest(&dir)?;
        assert!(manifest.iter().any(|f| f.path == "index.html"));
        assert!(manifest.iter().any(|f| f.path == "previews_glb/a.glb"));
        assert!(
            !manifest
                .iter()
                .any(|f| f.path.contains(".git") || f.path.contains("three"))
        );

        let bytes = create_incremental_zip(&dir, &manifest, &["index.html".to_string()])?;
        let cursor = Cursor::new(&bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "index.html"),
            "changed file should be in zip: {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n == MANIFEST_ENTRY),
            "manifest should be in zip: {:?}",
            names
        );
        assert!(
            !names.iter().any(|n| n == "previews_glb/a.glb"),
            "unchanged file should not be in zip: {:?}",
            names
        );
        assert!(
            !names
                .iter()
                .any(|n| n.contains(".git") || n.contains("three")),
            ".git/three must be excluded: {:?}",
            names
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn create_zip_missing_dir_fails() {
        let missing = std::env::temp_dir().join(format!("ks_push_missing_{}", std::process::id()));
        let manifest = vec![DeployFile {
            path: "index.html".to_string(),
            size: 0,
            hash: "abc".to_string(),
        }];
        assert!(create_incremental_zip(&missing, &manifest, &["index.html".to_string()]).is_err());
    }
}
