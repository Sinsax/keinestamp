use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub blender_path: Option<String>,
    #[serde(default)]
    pub blender_version: Option<String>,
    #[serde(default)]
    pub blender_versions: Option<String>,
    #[serde(default)]
    pub deploy_server_url: Option<String>,
    #[serde(default)]
    pub deploy_token: Option<String>,
    #[serde(default)]
    pub lore_server_url: Option<String>,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub close_to_tray: Option<bool>,
    #[serde(default)]
    pub auto_restore_project: bool,
    #[serde(default)]
    pub lore_identity: Option<String>,
    /// 服务开关：网页部署 / loreserver（false = 关闭，测试与远端列表按此启用）
    #[serde(default)]
    pub deploy_enabled: bool,
    #[serde(default)]
    pub lore_enabled: bool,
}

impl AppConfig {
    pub fn load() -> Self {
        Self::load_from(&default_app_config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        self.save_to(&default_app_config_path());
    }

    pub fn save_to(&self, path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let tmp = path.with_extension(format!(
                "tmp{}_{}",
                std::process::id(),
                path.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            ));
            if std::fs::write(&tmp, &content).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            blender_path: None,
            blender_version: None,
            blender_versions: None,
            deploy_server_url: None,
            deploy_token: None,
            lore_server_url: None,
            auto_connect: false,
            close_to_tray: None,
            auto_restore_project: false,
            lore_identity: None,
            deploy_enabled: false,
            lore_enabled: false,
        }
    }
}

pub fn get_server_toggles() -> (bool, bool) {
    let cfg = AppConfig::load();
    (cfg.deploy_enabled, cfg.lore_enabled)
}

pub fn set_server_toggles(deploy_enabled: bool, lore_enabled: bool) {
    let mut cfg = AppConfig::load();
    cfg.deploy_enabled = deploy_enabled;
    cfg.lore_enabled = lore_enabled;
    cfg.save();
}

pub fn get_blender_path() -> Option<PathBuf> {
    let cfg = AppConfig::load();
    cfg.blender_path.as_deref().map(PathBuf::from)
}

pub fn set_blender_path(path: &str) {
    let mut cfg = AppConfig::load();
    cfg.blender_path = if path.is_empty() { None } else { Some(path.to_string()) };
    cfg.save();
}

pub fn get_blender_versions() -> Vec<String> {
    let cfg = AppConfig::load();
    cfg.blender_versions
        .as_deref()
        .map(|s| s.split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect())
        .unwrap_or_default()
}

pub fn set_blender_versions(versions: &[String]) {
    let mut cfg = AppConfig::load();
    cfg.blender_versions = Some(versions.join(","));
    cfg.save();
}

pub fn get_close_to_tray() -> Option<bool> {
    let cfg = AppConfig::load();
    cfg.close_to_tray
}

pub fn set_close_to_tray(close_to_tray: Option<bool>) {
    let mut cfg = AppConfig::load();
    cfg.close_to_tray = close_to_tray;
    cfg.save();
}

pub fn get_deploy_server_url() -> Option<String> {
    AppConfig::load().deploy_server_url
}

pub fn set_deploy_server_url(url: &str) {
    let mut cfg = AppConfig::load();
    cfg.deploy_server_url = if url.is_empty() { None } else { Some(url.to_string()) };
    cfg.save();
}

pub fn get_deploy_token() -> Option<String> {
    AppConfig::load().deploy_token
}

pub fn set_deploy_token(token: &str) {
    let mut cfg = AppConfig::load();
    cfg.deploy_token = if token.is_empty() { None } else { Some(token.to_string()) };
    cfg.save();
}

pub fn get_auto_connect() -> bool {
    AppConfig::load().auto_connect
}

/// loreserver 地址（局域网备份链路）；未配置时回退本机默认 41337。
pub fn get_lore_server_url() -> String {
    AppConfig::load()
        .lore_server_url
        .unwrap_or_else(|| crate::lore_backend::LORE_SERVER_URL.to_string())
}

pub fn set_lore_server_url(url: &str) {
    let mut cfg = AppConfig::load();
    cfg.lore_server_url = if url.trim().is_empty() { None } else { Some(url.trim().to_string()) };
    cfg.save();
}

pub fn get_auto_restore_project() -> bool {
    AppConfig::load().auto_restore_project
}

pub fn set_auto_restore_project(v: bool) {
    let mut cfg = AppConfig::load();
    cfg.auto_restore_project = v;
    cfg.save();
}

/// 提交身份（程序级全局；未配置 → 默认 keinestamp@local）
pub fn get_lore_identity() -> String {
    AppConfig::load()
        .lore_identity
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "keinestamp@local".to_string())
}

pub fn set_lore_identity(identity: &str) {
    let mut cfg = AppConfig::load();
    cfg.lore_identity = if identity.trim().is_empty() {
        None
    } else {
        Some(identity.trim().to_string())
    };
    cfg.save();
}

pub fn set_auto_connect(v: bool) {
    let mut cfg = AppConfig::load();
    cfg.auto_connect = v;
    cfg.save();
}

pub fn default_app_config_path() -> PathBuf {
    let (base, dir_name) = if cfg!(target_os = "windows") {
        (
            std::env::var("APPDATA")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            "Keinestamp",
        )
    } else {
        (
            std::env::var("XDG_DATA_HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var("HOME")
                        .ok()
                        .map(|h| PathBuf::from(h).join(".local").join("share"))
                })
                .unwrap_or_else(|| PathBuf::from(".")),
            "keinestamp",
        )
    };
    base.join(dir_name).join("app-config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_path_exists_or_creatable() {
        let path = default_app_config_path();
        assert!(path.to_string_lossy().contains("app-config.toml"));
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("keinestamp_app_config_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.toml");

        let cfg = AppConfig { blender_path: Some("/path/to/blender".to_string()), blender_version: None, blender_versions: None, deploy_server_url: None, deploy_token: None, lore_server_url: Some("lore://127.0.0.1:41337".to_string()), auto_connect: false, close_to_tray: Some(true), auto_restore_project: false, lore_identity: None, deploy_enabled: false, lore_enabled: false };
        cfg.save_to(&path);

        let loaded = AppConfig::load_from(&path);
assert_eq!(loaded.blender_path, Some("/path/to/blender".to_string()));
    assert_eq!(loaded.blender_version, None);
    assert_eq!(loaded.blender_versions, None);
    assert_eq!(loaded.deploy_server_url, None);
    assert_eq!(loaded.deploy_token, None);
    assert_eq!(loaded.lore_server_url, Some("lore://127.0.0.1:41337".to_string()));
    assert_eq!(loaded.close_to_tray, Some(true));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_missing_file() {
        let loaded = AppConfig::load_from(std::path::Path::new("/nonexistent/path/app-config.toml"));
assert_eq!(loaded.blender_path, None);
    assert_eq!(loaded.blender_version, None);
    assert_eq!(loaded.blender_versions, None);
    }

    #[test]
    fn test_set_blender_path() {
        let dir = std::env::temp_dir().join(format!("keinestamp_app_set_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.toml");

        let mut cfg = AppConfig::default();
        cfg.blender_path = Some("/usr/bin/blender".to_string());
        cfg.save_to(&path);

        let loaded = AppConfig::load_from(&path);
        assert_eq!(loaded.blender_path, Some("/usr/bin/blender".to_string()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_lore_identity_roundtrip() {
        let dir = std::env::temp_dir().join(format!("keinestamp_app_id_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.toml");

        // 默认 → keinestamp@local
        let mut cfg = AppConfig::default();
        cfg.save_to(&path);
        let loaded = AppConfig::load_from(&path);
        assert_eq!(
            loaded.lore_identity.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "keinestamp@local".to_string()),
            "keinestamp@local"
        );
        // 设置后持久化
        let mut cfg2 = AppConfig::load_from(&path);
        cfg2.lore_identity = Some("para@local".to_string());
        cfg2.save_to(&path);
        let loaded2 = AppConfig::load_from(&path);
        assert_eq!(loaded2.lore_identity, Some("para@local".to_string()));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}