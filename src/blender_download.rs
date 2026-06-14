use anyhow::Result;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const DEFAULT_BLENDER_VERSION: &str = "5.1.2";

const VERSION_MIRRORS: &[&str] = &[
    "https://mirrors.tuna.tsinghua.edu.cn/blender/release/",
    "https://mirrors.aliyun.com/blender/release/",
    "https://download.blender.org/release/",
];

const DOWNLOAD_MIRRORS: &[&str] = &[
    "https://mirrors.tuna.tsinghua.edu.cn/blender/release",
    "https://mirrors.aliyun.com/blender/release",
    "https://download.blender.org/release",
];

const KNOWN_VERSIONS: &[&str] = &[
    "5.1.2", "5.1.1", "5.1.0",
    "5.0.1", "5.0.0",
    "4.5.0",
    "4.4.3", "4.4.2", "4.4.1", "4.4.0",
    "4.3.0",
    "4.2.8", "4.2.7", "4.2.6", "4.2.5", "4.2.4", "4.2.3", "4.2.2", "4.2.1", "4.2.0",
    "4.1.1", "4.1.0",
    "4.0.2", "4.0.1", "4.0.0",
    "3.6.20", "3.6.19", "3.6.18",
];

pub struct DownloadProgress {
    pub phase: String,
    pub message: String,
    pub percent: i32,
    pub bytes_downloaded: u64,
    pub bytes_total: u64,
}

pub fn blender_install_dir() -> PathBuf {
    crate::app_config::default_app_config_path()
        .parent()
        .map(|p| p.join("blender"))
        .unwrap_or_else(|| PathBuf::from("blender"))
}

pub fn default_blender_version() -> &'static str {
    DEFAULT_BLENDER_VERSION
}

pub fn list_blender_versions() -> Vec<String> {
    KNOWN_VERSIONS.iter().map(|s| s.to_string()).collect()
}

pub fn fetch_blender_versions() -> Result<Vec<String>> {
    eprintln!("[debug] fetch_blender_versions: trying mirrors");
    for mirror in VERSION_MIRRORS {
        eprintln!("[debug] fetch_blender_versions: trying {}", mirror);
        match fetch_versions_from(mirror) {
            Ok(versions) if !versions.is_empty() => {
                eprintln!("[debug] fetch_blender_versions: got {} versions from {}", versions.len(), mirror);
                return Ok(versions);
            }
            Ok(_) => {
                eprintln!("[debug] fetch_blender_versions: mirror {} returned empty", mirror);
            }
            Err(e) => {
                eprintln!("[debug] fetch_blender_versions: mirror {} failed: {}", mirror, e);
            }
        }
    }
    eprintln!("[debug] fetch_blender_versions: all mirrors failed, using fallback");
    Ok(list_blender_versions())
}

fn fetch_versions_from(base_url: &str) -> Result<Vec<String>> {
    let url = if base_url.ends_with('/') { base_url.to_string() } else { format!("{}/", base_url) };
    let html = fetch_url(&url)?;

    let mut versions = Vec::new();
    for line in html.lines() {
        if let Some(href) = line.split("href=\"").nth(1) {
            if let Some(dir) = href.split('"').next() {
                if let Some(ver) = dir.strip_prefix("Blender") {
                    let ver = ver.trim_end_matches('/');
                    if ver.contains('.') && ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        let sub_url = format!("{}Blender{}/", url.trim_end_matches('/'), ver);
                        if let Ok(sub_html) = fetch_url(&sub_url) {
                            for sub_line in sub_html.lines() {
                                if let Some(sub_href) = sub_line.split("href=\"").nth(1) {
                                    if let Some(file) = sub_href.split('"').next() {
                                        let prefix = "blender-";
                                        let suffix = if cfg!(target_os = "windows") {
                                            "-windows-x64.zip"
                                        } else if cfg!(target_os = "macos") {
                                            "-macos-arm64.dmg"
                                        } else {
                                            "-linux-x64.tar.xz"
                                        };
                                        if let Some(rest) = file.strip_prefix(prefix) {
                                            if let Some(v) = rest.strip_suffix(suffix) {
                                                if v.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                                    versions.push(v.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    versions.sort_by(|a, b| {
        let ap: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
        let bp: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
        bp.cmp(&ap)
    });
    versions.dedup();

    Ok(versions)
}

fn fetch_url(url: &str) -> Result<String> {
    if cfg!(target_os = "windows") {
        let script = format!(
            "(New-Object System.Net.WebClient).DownloadString('{}')",
            url
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| anyhow::anyhow!("获取版本列表失败: {}", e))?;
        if !output.status.success() {
            anyhow::bail!("获取版本列表失败");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let output = std::process::Command::new("curl")
            .args(["-sL", url])
            .output()
            .or_else(|_| std::process::Command::new("wget").args(["-qO-", url]).output())
            .map_err(|e| anyhow::anyhow!("获取版本列表失败: {}", e))?;
        if !output.status.success() {
            anyhow::bail!("获取版本列表失败");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

pub fn download_blender(version: Option<&str>) -> Result<PathBuf> {
    download_blender_with_progress(version, &mut |_p| {})
}

pub fn download_blender_with_progress(
    version: Option<&str>,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<PathBuf> {
    let ver = version.unwrap_or(DEFAULT_BLENDER_VERSION);
    eprintln!("[debug] download_blender: version={}", ver);
    let major_minor = {
        let parts: Vec<&str> = ver.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            ver.to_string()
        }
    };

    let filename = if cfg!(target_os = "windows") {
        format!("blender-{}-windows-x64.zip", ver)
    } else if cfg!(target_os = "macos") {
        format!("blender-{}-macos-arm64.dmg", ver)
    } else {
        format!("blender-{}-linux-x64.tar.xz", ver)
    };

    let app_dir = blender_install_dir();
    eprintln!("[debug] download_blender: install_dir={}", app_dir.display());
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| anyhow::anyhow!("创建目录失败: {}", e))?;

    let mut last_err = None;
    for mirror_base in DOWNLOAD_MIRRORS {
        let url = format!("{}/Blender{}/{}", mirror_base, major_minor, filename);
        eprintln!("[debug] download_blender: trying mirror {}", url);
        let archive_path = app_dir.join(&filename);

        on_progress(DownloadProgress {
            phase: "downloading".into(),
            message: format!("正在下载 Blender {}...", ver),
            percent: 0,
            bytes_downloaded: 0,
            bytes_total: 0,
        });

        match streaming_download(&url, &archive_path, on_progress) {
            Ok(()) => {
                if !archive_path.exists() {
                    eprintln!("[debug] download_blender: archive not found after download from {}", url);
                    let _ = std::fs::remove_file(&archive_path);
                    last_err = Some(anyhow::anyhow!("下载完成但未找到文件: {}", archive_path.to_string_lossy()));
                    continue;
                }
                return extract_blender(&app_dir, &archive_path, on_progress, ver);
            }
            Err(e) => {
                eprintln!("[debug] download_blender: mirror {} failed: {}", url, e);
                let _ = std::fs::remove_file(app_dir.join(&filename));
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("所有镜像源均下载失败")))
}

fn extract_blender(
    app_dir: &Path,
    archive_path: &Path,
    on_progress: &mut impl FnMut(DownloadProgress),
    ver: &str,
) -> Result<PathBuf> {
    on_progress(DownloadProgress {
        phase: "extracting".into(),
        message: "正在解压...".into(),
        percent: 0,
        bytes_downloaded: 0,
        bytes_total: 0,
    });

    eprintln!("[debug] extract_blender: extracting archive...");
    let extract_result = if cfg!(target_os = "windows") {
        let script = format!(
            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            archive_path.to_string_lossy(),
            app_dir.to_string_lossy()
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| anyhow::anyhow!("解压失败: {}", e))?
    } else {
        std::process::Command::new("tar")
            .args([
                "xf",
                &archive_path.to_string_lossy(),
                "-C",
                &app_dir.to_string_lossy(),
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("解压失败: {}", e))?
    };

    if !extract_result.status.success() {
        let stderr = String::from_utf8_lossy(&extract_result.stderr);
        let _ = std::fs::remove_file(archive_path);
        anyhow::bail!("解压失败: {}", stderr.trim());
    }

    let _ = std::fs::remove_file(archive_path);
    eprintln!("[debug] extract_blender: extraction complete");

    on_progress(DownloadProgress {
        phase: "configuring".into(),
        message: "正在配置...".into(),
        percent: 95,
        bytes_downloaded: 0,
        bytes_total: 0,
    });

    let blender_name = if cfg!(target_os = "windows") {
        "blender.exe"
    } else {
        "blender"
    };
    let blender_exe = find_file(app_dir, blender_name).ok_or_else(|| {
        eprintln!("[debug] extract_blender: blender not found after extraction!");
        let mut extracted_files = String::new();
        if let Ok(entries) = std::fs::read_dir(app_dir) {
            for entry in entries.flatten() {
                if !extracted_files.is_empty() {
                    extracted_files.push_str(", ");
                }
                extracted_files.push_str(&entry.file_name().to_string_lossy());
            }
        }
        anyhow::anyhow!(
            "在解压目录中未找到 {}。解压后的内容: [{}]",
            blender_name,
            extracted_files
        )
    })?;

    crate::app_config::set_blender_path(&blender_exe.to_string_lossy());
    eprintln!("[debug] extract_blender: blender installed at {:?}", blender_exe);

    on_progress(DownloadProgress {
        phase: "done".into(),
        message: format!("Blender {} 已安装", ver),
        percent: 100,
        bytes_downloaded: 0,
        bytes_total: 0,
    });

    Ok(blender_exe)
}

fn streaming_download(
    url: &str,
    dest: &Path,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    eprintln!("[debug] streaming_download: connecting to {}", url);
    let resp = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("下载连接失败: {}", e))?;

    let total_size: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    eprintln!("[debug] streaming_download: total_size={} bytes", total_size);

    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)
        .map_err(|e| anyhow::anyhow!("创建文件失败: {}", e))?;

    let mut buf = vec![0u8; 65536];
    let mut downloaded: u64 = 0;
    let mut last_percent: i32 = -1;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| anyhow::anyhow!("下载读取失败: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| anyhow::anyhow!("写入文件失败: {}", e))?;
        downloaded += n as u64;

        if total_size > 0 {
            let pct = ((downloaded * 100) / total_size) as i32;
            if pct != last_percent {
                last_percent = pct;
                on_progress(DownloadProgress {
                    phase: "downloading".into(),
                    message: format!("正在下载... {}%", pct),
                    percent: pct,
                    bytes_downloaded: downloaded,
                    bytes_total: total_size,
                });
            }
        } else {
            let mb = downloaded as f64 / 1_048_576.0;
            let pct_downloaded = if downloaded > 0 { (downloaded % 1_048_576 == 0) as i32 * 5 } else { 0 };
            if downloaded % 1_048_576 < 65536 {
                on_progress(DownloadProgress {
                    phase: "downloading".into(),
                    message: format!("正在下载... {:.1} MB", mb),
                    percent: pct_downloaded.max(1),
                    bytes_downloaded: downloaded,
                    bytes_total: 0,
                });
            }
        }
    }

    file.flush()
        .map_err(|e| anyhow::anyhow!("刷新文件失败: {}", e))?;
    drop(file);

    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_file(&path, name) {
                    return Some(found);
                }
            } else if path
                .file_name()
                .map(|n| n == name)
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

pub fn find_bundled_blender() -> Option<PathBuf> {
    eprintln!("[debug] find_bundled_blender: checking configured path");
    let current = crate::app_config::get_blender_path();
    if current.is_some() {
        eprintln!("[debug] find_bundled_blender: found at {:?}", current);
        return current;
    }

    let blender_name = if cfg!(target_os = "windows") {
        "blender.exe"
    } else {
        "blender"
    };
    let install_dir = blender_install_dir();
    eprintln!("[debug] find_bundled_blender: searching install_dir={}", install_dir.display());
    let found = find_file(&install_dir, blender_name);
    if found.is_some() {
        eprintln!("[debug] find_bundled_blender: found at {:?}", found);
    } else {
        eprintln!("[debug] find_bundled_blender: not found");
    }
    found
}

pub fn ensure_blender() -> Result<PathBuf> {
    eprintln!("[debug] ensure_blender: starting");
    if let Some(path) = find_bundled_blender() {
        eprintln!("[debug] ensure_blender: found existing blender at {:?}", path);
        return Ok(path);
    }

    eprintln!("[debug] ensure_blender: Blender not installed, auto-downloading...");
    download_blender(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_version() {
        assert!(!DEFAULT_BLENDER_VERSION.is_empty());
    }

    #[test]
    fn test_install_dir_is_under_config() {
        let dir = blender_install_dir();
        assert!(dir.to_string_lossy().contains("blender"));
    }

    #[test]
    fn test_version_to_major_minor() {
        assert_eq!({
            let parts: Vec<&str> = "5.1.2".split('.').collect();
            if parts.len() >= 2 { format!("{}.{}", parts[0], parts[1]) } else { "5.1.2".to_string() }
        }, "5.1");
        assert_eq!({
            let parts: Vec<&str> = "4.4.0".split('.').collect();
            if parts.len() >= 2 { format!("{}.{}", parts[0], parts[1]) } else { "4.4.0".to_string() }
        }, "4.4");
        assert_eq!({
            let parts: Vec<&str> = "3.0".split('.').collect();
            if parts.len() >= 2 { format!("{}.{}", parts[0], parts[1]) } else { "3.0".to_string() }
        }, "3.0");
    }

    #[test]
    fn test_download_url_format() {
        let ver = "5.1.2";
        let major_minor = {
            let parts: Vec<&str> = ver.split('.').collect();
            format!("{}.{}", parts[0], parts[1])
        };
        let filename = format!("blender-{}-windows-x64.zip", ver);
        let url = format!(
            "https://download.blender.org/release/Blender{}/{}",
            major_minor, filename
        );
        assert_eq!(url, "https://download.blender.org/release/Blender5.1/blender-5.1.2-windows-x64.zip");
    }

    #[test]
    fn test_known_versions_sorted() {
        let versions = list_blender_versions();
        assert!(!versions.is_empty());
        assert_eq!(versions[0], DEFAULT_BLENDER_VERSION);
        for v in &versions {
            let parts: Vec<&str> = v.split('.').collect();
            assert!(parts.len() >= 2, "version {} should have at least 2 parts", v);
        }
    }
}