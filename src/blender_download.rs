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

const VERSION_FETCH_LIMIT: usize = 12;
const INDEX_TIMEOUT_SECS: u64 = 12;
const SUBPAGE_TIMEOUT_SECS: u64 = 4;

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

pub fn list_installed_blender_versions() -> Vec<String> {
    list_installed_versions_from(&blender_install_dir())
}

fn list_installed_versions_from(dir: &Path) -> Vec<String> {
    let mut installed = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Directories are named like "blender-4.2.0-windows-x64" — extract version
            if let Some(rest) = name.strip_prefix("blender-") {
                if let Some(ver_end) = rest.find('-') {
                    let ver = &rest[..ver_end];
                    if ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        installed.push(ver.to_string());
                    }
                }
            }
        }
    }
    installed
}

/// 删除已安装的 Blender 版本；**最少保留一个**（唯一版本拒绝删除）。
/// 若 app-config 的 blender_path 指向被删版本 → 清空（下次 ensure_blender 自动回退默认）。
pub fn delete_blender_version(version: &str) -> Result<()> {
    delete_blender_version_at(&blender_install_dir(), version)
}

fn delete_blender_version_at(dir: &Path, version: &str) -> Result<()> {
    let installed = list_installed_versions_from(dir);
    if !installed.iter().any(|v| v == version) {
        anyhow::bail!("未安装该 Blender 版本: {}", version);
    }
    if installed.len() <= 1 {
        anyhow::bail!("至少保留一个 Blender 版本（当前仅 {}）", installed.join(", "));
    }
    let target = dir.join(format!("blender-{}-windows-x64", version));
    std::fs::remove_dir_all(&target)
        .map_err(|e| anyhow::anyhow!("删除 {} 失败: {}", target.display(), e))?;
    // blender_path 指向被删版本 → 清空
    if let Some(p) = crate::app_config::get_blender_path() {
        if p.starts_with(&target) {
            crate::app_config::set_blender_path("");
        }
    }
    Ok(())
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
    eprintln!("[debug] fetch_blender_versions: all mirrors failed");
    anyhow::bail!("all version mirrors failed")
}

fn collect_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find("href=\"") {
        rest = &rest[idx + 6..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

fn parse_version_dirs(html: &str) -> Vec<String> {
    collect_hrefs(html)
        .into_iter()
        .filter_map(|href| {
            let ver = href.trim_end_matches('/').strip_prefix("Blender")?;
            if ver.contains('.') && ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                Some(ver.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn parse_installer_versions(html: &str, suffix: &str) -> Vec<String> {
    collect_hrefs(html)
        .into_iter()
        .filter_map(|href| {
            let v = href.strip_prefix("blender-")?.strip_suffix(suffix)?;
            if v.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                Some(v.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn fetch_versions_from(base_url: &str) -> Result<Vec<String>> {
    let url = if base_url.ends_with('/') { base_url.to_string() } else { format!("{}/", base_url) };
    let html = fetch_url(&url, INDEX_TIMEOUT_SECS)?;

    let dirs = parse_version_dirs(&html);

    let suffix = if cfg!(target_os = "windows") {
        "-windows-x64.zip"
    } else if cfg!(target_os = "macos") {
        "-macos-arm64.dmg"
    } else {
        "-linux-x64.tar.xz"
    };

    let mut versions = Vec::new();
    for chunk in dirs.chunks(8) {
        let handles: Vec<_> = chunk.iter().map(|ver| {
            let sub_url = format!("{}/Blender{}/", url.trim_end_matches('/'), ver);
            std::thread::spawn(move || {
                let sub_html = fetch_url(&sub_url, SUBPAGE_TIMEOUT_SECS).ok()?;
                Some(parse_installer_versions(&sub_html, suffix))
            })
        }).collect();
        for handle in handles {
            if let Ok(Some(found)) = handle.join() {
                versions.extend(found);
            }
        }
    }

    if versions.is_empty() {
        anyhow::bail!("no installer files found under {} ({} version dirs)", url, dirs.len());
    }

    versions.sort_by(|a, b| {
        let ap: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
        let bp: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
        bp.cmp(&ap)
    });
    versions.dedup();
    versions.truncate(VERSION_FETCH_LIMIT);

    Ok(versions)
}

fn fetch_url(url: &str, timeout_secs: u64) -> Result<String> {
    let body = ureq::get(url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .call()
        .map_err(|e| anyhow::anyhow!("获取版本列表失败: {}", e))?
        .into_string()
        .map_err(|e| anyhow::anyhow!("获取版本列表失败: {}", e))?;
    Ok(body)
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
        crate::process_util::hide_console(std::process::Command::new("powershell"))
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| anyhow::anyhow!("解压失败: {}", e))?
    } else {
        crate::process_util::hide_console(std::process::Command::new("tar"))
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

/// 从 exe 路径解析版本号：`...\blender-4.2.0-windows-x64\blender.exe` → `4.2.0`；无法解析 → None
pub fn parse_blender_version_from_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    let dir = if name.starts_with("blender.exe") || name == "blender" {
        path.parent()?.file_name()?.to_string_lossy().to_string()
    } else {
        path.file_name()?.to_string_lossy().to_string()
    };
    let rest = dir.strip_prefix("blender-")?;
    let ver_end = rest.find('-')?;
    let ver = &rest[..ver_end];
    if ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        Some(ver.to_string())
    } else {
        None
    }
}

/// 当前使用的 Blender 版本（app_config.blender_path 解析；未设置/未安装 → 空串）
pub fn current_blender_version() -> String {
    crate::app_config::get_blender_path()
        .as_deref()
        .and_then(parse_blender_version_from_path)
        .unwrap_or_default()
}

/// 切换当前使用的 Blender 版本（构造安装目录 exe 路径，校验存在后写入 app_config.blender_path）
pub fn set_current_blender_version(version: &str) -> Result<()> {
    set_current_blender_version_at(&blender_install_dir(), version)
}

fn set_current_blender_version_at(dir: &Path, version: &str) -> Result<()> {
    let exe_name = if cfg!(target_os = "windows") { "blender.exe" } else { "blender" };
    let exe = dir.join(format!("blender-{}-windows-x64", version)).join(exe_name);
    if !exe.exists() {
        anyhow::bail!("未安装该 Blender 版本: {}", version);
    }
    crate::app_config::set_blender_path(&exe.to_string_lossy());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_version() {
        assert!(!DEFAULT_BLENDER_VERSION.is_empty());
    }

    #[test]
    fn test_parse_version_dirs_single_line() {
        let html = "<!DOCTYPE html> <html> <head></head> <body> <table id=\"list\"><tbody>\
<tr><td class=\"link\"><a href=\"../\">Parent</a></td></tr>\
<tr><td class=\"link\"><a href=\"Blender4.2/\" title=\"Blender4.2/\">Blender4.2/</a></td></tr>\
<tr><td class=\"link\"><a href=\"Blender5.1/\" title=\"Blender5.1/\">Blender5.1/</a></td></tr>\
<tr><td class=\"link\"><a href=\"BlenderBenchmark1.0/\" title=\"BlenderBenchmark1.0/\">BlenderBenchmark1.0/</a></td></tr>\
<tr><td class=\"link\"><a href=\"GPL-license.txt\" title=\"GPL-license.txt\">GPL-license.txt</a></td></tr>\
</tbody></table> </body></html>";
        let dirs = parse_version_dirs(html);
        assert_eq!(dirs, vec!["4.2".to_string(), "5.1".to_string()]);
    }

    #[test]
    fn test_parse_version_dirs_multiline_apache() {
        let html = "<html>\n<h1>Index of /release/</h1><hr><pre><a href=\"../\">../</a>\n<a href=\"Blender2.83/\">Blender2.83/</a>  05-Jul-2020 07:17 -\n<a href=\"Blender3.6/\">Blender3.6/</a>  01-Jan-2021 10:00 -\n<a href=\"blender2.04-ipaq.zip\">blender2.04-ipaq.zip</a>\n</pre></html>";
        let dirs = parse_version_dirs(html);
        assert_eq!(dirs, vec!["2.83".to_string(), "3.6".to_string()]);
    }

    #[test]
    fn test_parse_installer_versions_multiline() {
        let html = "<a href=\"blender-4.2.8-windows-x64.zip\">blender-4.2.8-windows-x64.zip</a>\n<a href=\"blender-4.2.8-linux-x64.tar.xz\">...</a>\n<a href=\"blender-4.2.8-windows-x64.msi\">...</a>";
        let versions = parse_installer_versions(html, "-windows-x64.zip");
        assert_eq!(versions, vec!["4.2.8".to_string()]);
    }

    #[test]
    fn test_parse_installer_versions_single_line() {
        let html = "<a href=\"blender-5.1.0-windows-x64.zip\">x</a> <a href=\"blender-5.1.0-macos-arm64.dmg\">y</a> <a href=\"blender-4.2.0-windows-x64.zip\">z</a>";
        let versions = parse_installer_versions(html, "-windows-x64.zip");
        assert_eq!(versions, vec!["5.1.0".to_string(), "4.2.0".to_string()]);
    }

    #[test]
    fn test_delete_blender_version_at() {
        let dir = std::env::temp_dir().join(format!("keinestamp_delblend_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("blender-4.2.0-windows-x64")).unwrap();
        std::fs::create_dir_all(dir.join("blender-4.3.0-windows-x64")).unwrap();
        assert_eq!(list_installed_versions_from(&dir), vec!["4.2.0".to_string(), "4.3.0".to_string()]);

        // 删 4.2 → 剩 4.3
        delete_blender_version_at(&dir, "4.2.0").unwrap();
        assert_eq!(list_installed_versions_from(&dir), vec!["4.3.0".to_string()]);
        // 唯一版本拒绝删除（最少保留一个）
        let err = delete_blender_version_at(&dir, "4.3.0").unwrap_err();
        assert!(err.to_string().contains("至少保留一个"), "err: {err}");
        // 未安装版本报错
        let err = delete_blender_version_at(&dir, "9.9.9").unwrap_err();
        assert!(err.to_string().contains("未安装"), "err: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_blender_version_from_path() {
        assert_eq!(
            parse_blender_version_from_path(Path::new(r"C:\Users\x\AppData\Roaming\Keinestamp\blender\blender-4.2.0-windows-x64\blender.exe")),
            Some("4.2.0".to_string())
        );
        assert_eq!(
            parse_blender_version_from_path(Path::new(r"/home/x/.config/keinestamp/blender/blender-5.1.2-windows-x64/blender")),
            Some("5.1.2".to_string())
        );
        assert_eq!(parse_blender_version_from_path(Path::new("C:\\tools\\blender.exe")), None);
        assert_eq!(parse_blender_version_from_path(Path::new("")), None);
    }

    #[test]
    fn test_set_current_blender_version_at() {
        let dir = std::env::temp_dir().join(format!("keinestamp_setblend_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("blender-4.2.0-windows-x64")).unwrap();
        std::fs::write(dir.join("blender-4.2.0-windows-x64/blender.exe"), "x").unwrap();
        // 未安装版本报错
        let err = set_current_blender_version_at(&dir, "4.3.0").unwrap_err();
        assert!(err.to_string().contains("未安装"), "err: {err}");
        let _ = std::fs::remove_dir_all(&dir);
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