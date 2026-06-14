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
}

impl AppConfig {
    pub fn load() -> Self {
        Self::load_from(&default_app_config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or(Self {
                blender_path: None,
                blender_version: None,
                blender_versions: None,
            })
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

pub fn default_app_config_path() -> PathBuf {
    let (base, dir_name) = if cfg!(target_os = "windows") {
        (
            std::env::var("APPDATA")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            "SakiStep",
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
            "saki-step",
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
        let dir = std::env::temp_dir().join(format!("saki_app_config_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.toml");

        let cfg = AppConfig { blender_path: Some("/path/to/blender".to_string()), blender_version: None, blender_versions: None };
        cfg.save_to(&path);

        let loaded = AppConfig::load_from(&path);
assert_eq!(loaded.blender_path, Some("/path/to/blender".to_string()));
    assert_eq!(loaded.blender_version, None);
    assert_eq!(loaded.blender_versions, None);

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
        let dir = std::env::temp_dir().join(format!("saki_app_set_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.toml");

        let mut cfg = AppConfig { blender_path: None, blender_version: None, blender_versions: None };
        cfg.blender_path = Some("/usr/bin/blender".to_string());
        cfg.save_to(&path);

        let loaded = AppConfig::load_from(&path);
        assert_eq!(loaded.blender_path, Some("/usr/bin/blender".to_string()));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}