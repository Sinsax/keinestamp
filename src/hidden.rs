use std::path::Path;
use anyhow::Result;

/// web 产物相对目录（html_gen 输出 + timeline.json；位于 .keinestamp/ 私有目录内）
pub const WEB_DIR: &str = ".keinestamp/web/";
/// Keinestamp 私有目录（.keinestamp.toml、web/ 等；应用层统一隐藏）
pub const PRIVATE_DIR: &str = ".keinestamp/";
/// Lore 原生忽略文件（项目级，被提交跟随历史；由 Lore 在 status/stage 原生过滤）
pub const LOREIGNORE: &str = ".loreignore";

/// 应用层隐藏（程序私有目录 + .loreignore 自身，UI 变更面板不显示）。
/// `.keinestamp/` 已由 loreignore 预设整体忽略（init 时 ensure_loreignore 写入，lore 原生过滤）——
/// 应用层 hidden 过滤保留作为双保险（用户删除 loreignore 行时仍隐藏程序产物）。
/// `.loreignore` 自身由 record 强制携带自动入库（"自动记录"，不显示在 UI）。
pub fn load_hidden(_project_root: &Path) -> Vec<String> {
    vec![PRIVATE_DIR.to_string(), LOREIGNORE.to_string()]
}

/// .loreignore 规则读取（项目级用户忽略；Lore 原生在 status/stage 过滤，无需应用层匹配）。
/// 防御：strip UTF-8 BOM（实测 PowerShell 写 UTF8 带 BOM 会导致 lore 首行规则失效）。
pub fn load_ignore_rules(project_root: &Path) -> Vec<String> {
    let path = project_root.join(LOREIGNORE);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

/// .loreignore 规则写入（无 BOM；文件被提交，规则随仓库历史——与 gitignore 同语义）
pub fn save_ignore_rules(project_root: &Path, rules: &[String]) -> Result<()> {
    let mut content = String::new();
    for r in rules {
        let t = r.trim();
        if !t.is_empty() {
            content.push_str(t);
            content.push('\n');
        }
    }
    std::fs::write(project_root.join(LOREIGNORE), content)?;
    Ok(())
}

/// 前缀匹配 + glob（`*`/`?` 通配）匹配。
/// - 含 `*` 或 `?` 的规则（如 `*.blend`）：通配匹配；规则不含 `/` 时只匹配任意层级的文件名
/// - 其他规则（如 `.keinestamp/`）：前缀匹配
pub fn is_hidden(path: &str, list: &[String]) -> bool {
    list.iter().any(|h| rule_matches(h, path))
}

fn rule_matches(rule: &str, path: &str) -> bool {
    // Windows 反斜杠路径归一化为 `/`，保证目录 glob 规则（如 `cache/*`）可匹配
    let rule = rule.replace('\\', "/");
    let path = path.replace('\\', "/");
    if rule.contains('*') || rule.contains('?') {
        let rule_segs: Vec<&str> = rule.split('/').collect();
        if rule_segs.len() == 1 {
            // 无 / 的规则：匹配任意层级下的文件名（如 `*.blend` 匹配 `models/x.blend`）
            let file = path.rsplit('/').next().unwrap_or(&path);
            wildcard_match(&rule, file)
        } else {
            // 含 / 的规则：段数必须一致（`cache/*` 匹配 `cache/x`，不匹配 `cache/a/b`）
            let path_segs: Vec<&str> = path.split('/').collect();
            path_segs.len() == rule_segs.len()
                && rule_segs.iter().zip(path_segs.iter()).all(|(r, p)| wildcard_match(r, p))
        }
    } else {
        path == rule || path.starts_with(&rule)
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
    fn test_load_hidden_presets_include_loreignore() {
        let dir = std::env::temp_dir().join(format!("keinestamp_hp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hidden = load_hidden(&dir);
        assert!(hidden.contains(&PRIVATE_DIR.to_string()));
        assert!(hidden.contains(&LOREIGNORE.to_string()));
        assert!(is_hidden(".loreignore", &hidden));
        assert!(is_hidden(".keinestamp/web/timeline.json", &hidden));
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
    fn test_load_ignore_rules_skips_bom_and_comments() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_ig_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 带 BOM 写入（模拟 PowerShell 场景）——读取时应 strip
        let with_bom = format!("\u{feff}*.blend\n.keinestamp/.keinestamp.toml\n# 注释\n\ncache/\n");
        std::fs::write(dir.join(LOREIGNORE), with_bom).unwrap();
        let rules = load_ignore_rules(&dir);
        assert_eq!(rules, vec!["*.blend", ".keinestamp/.keinestamp.toml", "cache/"]);
        std::fs::remove_dir_all(&dir).unwrap();
        Ok(())
    }

    #[test]
    fn test_save_ignore_rules_roundtrip() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_ig_sv_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        save_ignore_rules(&dir, &["*.blend".to_string(), "cache/".to_string()])?;
        let raw = std::fs::read_to_string(dir.join(LOREIGNORE)).unwrap();
        // 无 BOM
        assert!(!raw.starts_with('\u{feff}'));
        assert_eq!(load_ignore_rules(&dir), vec!["*.blend", "cache/"]);
        std::fs::remove_dir_all(&dir).unwrap();
        Ok(())
    }
}
