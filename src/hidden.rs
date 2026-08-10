use std::path::Path;
use anyhow::Result;
use crate::app_config::AppConfig;

pub const WEB_DIR: &str = "web/";

pub fn load_hidden(_project_root: &Path) -> Vec<String> {
    let mut list = AppConfig::load().hidden_paths.unwrap_or_default();
    if !list.iter().any(|h| h == WEB_DIR) {
        list.insert(0, WEB_DIR.to_string());
    }
    list
}

pub fn save_hidden(_project_root: &Path, list: &[String]) -> Result<()> {
    let mut list = list.to_vec();
    if !list.iter().any(|h| h == WEB_DIR) {
        list.insert(0, WEB_DIR.to_string());
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
}
