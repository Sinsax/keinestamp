use std::path::Path;
use anyhow::Result;
use crate::app_config::AppConfig;

pub const WEB_DIR: &str = "web/";
/// 预设隐藏：web/（部署产物）与 .keinestamp.toml（含 access_token，防误提交）
const PRESET_HIDDEN: [&str; 2] = [WEB_DIR, ".keinestamp.toml"];

pub fn load_hidden(_project_root: &Path) -> Vec<String> {
    let mut list = AppConfig::load().hidden_paths.unwrap_or_default();
    for preset in PRESET_HIDDEN {
        if !list.iter().any(|h| h == preset) {
            list.insert(0, preset.to_string());
        }
    }
    list
}

pub fn save_hidden(_project_root: &Path, list: &[String]) -> Result<()> {
    let mut list = list.to_vec();
    for preset in PRESET_HIDDEN {
        if !list.iter().any(|h| h == preset) {
            list.insert(0, preset.to_string());
        }
    }
    let mut cfg = AppConfig::load();
    cfg.hidden_paths = Some(list);
    cfg.save();
    Ok(())
}

pub fn is_hidden(path: &str, list: &[String]) -> bool {
    list.iter().any(|h| path == h || path.starts_with(h.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hidden_prefix_and_web_preset() {
        let list = vec!["web/".to_string(), "cache".to_string()];
        assert!(is_hidden("web/index.html", &list));
        assert!(is_hidden("cache/f.txt", &list));
        assert!(!is_hidden("models/a.blend", &list));
    }

    #[test]
    fn test_presets_always_present_and_hide_meta_config() {
        let dir = std::env::temp_dir().join(format!("keinestamp_hidden_preset_{}", std::process::id()));
        let list = load_hidden(&dir);
        assert!(list.iter().any(|h| h == "web/"));
        assert!(list.iter().any(|h| h == ".keinestamp.toml"));
        assert!(is_hidden(".keinestamp.toml", &list));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
