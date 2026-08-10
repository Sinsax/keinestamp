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

/// 前缀匹配 + glob（`*`/`?` 通配）匹配。
/// - 含 `*` 或 `?` 的规则（如 `*.blend`）：通配匹配；规则不含 `/` 时只匹配任意层级的文件名
/// - 其他规则（如 `web/`、`cache`）：前缀匹配
pub fn is_hidden(path: &str, list: &[String]) -> bool {
    list.iter().any(|h| rule_matches(h, path))
}

fn rule_matches(rule: &str, path: &str) -> bool {
    if rule.contains('*') || rule.contains('?') {
        let rule_segs: Vec<&str> = rule.split('/').collect();
        if rule_segs.len() == 1 {
            // 无 / 的规则：匹配任意层级下的文件名（如 `*.blend` 匹配 `models/x.blend`）
            let file = path.rsplit('/').next().unwrap_or(path);
            wildcard_match(rule, file)
        } else {
            // 含 / 的规则：段数必须一致（`cache/*` 匹配 `cache/x`，不匹配 `cache/a/b`）
            let path_segs: Vec<&str> = path.split('/').collect();
            path_segs.len() == rule_segs.len()
                && rule_segs.iter().zip(path_segs.iter()).all(|(r, p)| wildcard_match(r, p))
        }
    } else {
        path == rule || path.starts_with(rule)
    }
}

/// 经典贪心通配符匹配：`*` 匹配任意字符序列，`?` 匹配单字符。
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
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
    fn test_is_hidden_glob_ext() {
        let list = vec!["*.blend".to_string()];
        assert!(is_hidden("scene.blend", &list));
        assert!(is_hidden("models/x.blend", &list));
        assert!(!is_hidden("scene.blend1", &list));
        assert!(!is_hidden("scene.txt", &list));
    }

    #[test]
    fn test_is_hidden_glob_dir() {
        let list = vec!["cache/*".to_string()];
        assert!(is_hidden("cache/a.txt", &list));
        assert!(!is_hidden("cache/sub/b.txt", &list));
        assert!(!is_hidden("other/a.txt", &list));
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
