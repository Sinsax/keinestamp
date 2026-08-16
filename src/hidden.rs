use anyhow::Result;
use std::path::Path;

/// web 产物相对目录（html_gen 输出 + timeline.json；位于 .keinestamp/ 私有目录内）
pub const WEB_DIR: &str = ".keinestamp/web/";
/// Keinestamp 私有目录（.keinestamp.toml、web/ 等；应用层统一隐藏）
pub const PRIVATE_DIR: &str = ".keinestamp/";
/// Lore 原生忽略文件（项目级，被提交跟随历史；由 Lore 在 status/stage 原生过滤）
pub const LOREIGNORE: &str = ".loreignore";
/// 空提交 / 纯里程碑记录用的私有 marker（位于 .keinestamp/ 内，UI/时间线隐藏；
/// 每次空提交更新内容并通过 `--force stage` 制造可提交改动）
pub const EMPTY_MARKER: &str = ".keinestamp/milestone-marker";

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

/// lore `.loreignore` 语义匹配（docs/lore-commands.md:60，gitignore 风格，源码 filter.rs）：
/// - 规则按序求值、最后命中者生效；`!` 前缀取反
/// - 首 `/` 根锚定；尾 `/` 目录规则（任意层级的该目录名，根锚定时仅根级）
/// - 无 `/` 裸规则 = 任意层级段名（精确相等，非前缀——`record` 不匹配 `recording/x`）
/// - 含 `/` 规则 = 根级段前缀匹配（`**` 跨段递归、`*`/`?` 段内通配）
/// - 大小写不敏感（规则与路径均小写化后比较）
/// 供 `get_ignore_info` 计算权威忽略清单（变更面板灰显），不用于应用层隐藏。
pub fn is_ignored(path: &str, rules: &[String]) -> bool {
    let mut ignored = false;
    for raw in rules {
        let rule = raw.trim();
        if rule.is_empty() || rule.starts_with('#') {
            continue;
        }
        let (negated, rule) = match rule.strip_prefix('!') {
            Some(r) => (true, r.trim()),
            None => (false, rule),
        };
        if rule.is_empty() {
            continue;
        }
        if lore_rule_matches(rule, path) {
            ignored = !negated;
        }
    }
    ignored
}

fn lore_rule_matches(rule: &str, path: &str) -> bool {
    // 双方小写化（lore 大小写不敏感；中文文件名 to_lowercase 恒等）
    let path_norm = path.replace('\\', "/").to_lowercase();
    let rule_lower = rule.to_lowercase();
    let root_anchored = rule_lower.starts_with('/');
    let rule = rule_lower.trim_start_matches('/');
    let dir_rule = rule.ends_with('/');
    let rule = rule.trim_end_matches('/');
    if rule.is_empty() {
        return false;
    }
    let rule_segs: Vec<&str> = rule.split('/').collect();
    let segs: Vec<&str> = path_norm.split('/').collect();
    if dir_rule {
        if root_anchored {
            return segs.first().map(|s| *s == rule).unwrap_or(false);
        }
        return segs.iter().any(|s| *s == rule);
    }
    if root_anchored {
        return segments_prefix_match(&rule_segs, &segs);
    }
    if rule_segs.len() == 1 {
        if rule_segs[0] == "**" {
            return true;
        }
        return segs.iter().any(|s| segment_match(rule_segs[0], s));
    }
    segments_prefix_match(&rule_segs, &segs)
}

/// 段序列前缀匹配（规则段是路径段的前缀；`**` 匹配零或多个段）。
/// 规则耗尽即视为匹配——命中的是目录时其下全部路径随之忽略（与 git 语义一致）。
fn segments_prefix_match(pattern: &[&str], text: &[&str]) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if pattern[0] == "**" {
        for skip in 0..=text.len() {
            if segments_prefix_match(&pattern[1..], &text[skip..]) {
                return true;
            }
        }
        return false;
    }
    if text.is_empty() {
        return false;
    }
    segment_match(pattern[0], text[0]) && segments_prefix_match(&pattern[1..], &text[1..])
}

/// 单段匹配（`*`/`?` 通配；大小写不敏感——双方已小写化）
fn segment_match(pattern: &str, text: &str) -> bool {
    wildcard_match(pattern, text)
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
                && rule_segs
                    .iter()
                    .zip(path_segs.iter())
                    .all(|(r, p)| wildcard_match(r, p))
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
        assert_eq!(
            rules,
            vec!["*.blend", ".keinestamp/.keinestamp.toml", "cache/"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
        Ok(())
    }

    #[test]
    fn test_is_ignored_bare_rule_any_level() {
        let rules = vec!["record".to_string()];
        assert!(is_ignored("record/week1/x.png", &rules));
        assert!(is_ignored("a/record/x.png", &rules));
        assert!(is_ignored("record", &rules));
        // 段名精确相等：不误伤前缀
        assert!(!is_ignored("recording/x.png", &rules));
        assert!(!is_ignored("x.png", &rules));
    }

    #[test]
    fn test_is_ignored_dir_rule_any_level() {
        let rules = vec!["out/".to_string()];
        assert!(is_ignored("out/x.png", &rules));
        assert!(is_ignored("a/out/x.png", &rules));
        assert!(!is_ignored("outx/x.png", &rules));
    }

    #[test]
    fn test_is_ignored_root_anchored() {
        let rules = vec!["/out/".to_string(), "/top".to_string()];
        assert!(is_ignored("out/x.png", &rules));
        assert!(!is_ignored("a/out/x.png", &rules));
        assert!(is_ignored("top/a.png", &rules));
        assert!(!is_ignored("a/top/a.png", &rules));
    }

    #[test]
    fn test_is_ignored_case_insensitive() {
        let rules = vec!["RECORD".to_string()];
        assert!(is_ignored("record/x.png", &rules));
        assert!(is_ignored("Record/X.png", &rules));
    }

    #[test]
    fn test_is_ignored_double_star_recursive() {
        let rules = vec!["cache/**".to_string()];
        assert!(is_ignored("cache/a.png", &rules));
        assert!(is_ignored("cache/a/b/c.png", &rules));
        let rules2 = vec!["**/temp".to_string()];
        assert!(is_ignored("temp", &rules2));
        assert!(is_ignored("a/temp", &rules2));
        assert!(is_ignored("a/b/temp", &rules2));
        let rules3 = vec!["a/**/b".to_string()];
        assert!(is_ignored("a/b", &rules3));
        assert!(is_ignored("a/x/b", &rules3));
        assert!(is_ignored("a/x/y/b", &rules3));
    }

    #[test]
    fn test_is_ignored_negation_last_wins() {
        let rules = vec!["*.tmp".to_string(), "!keep.tmp".to_string()];
        assert!(is_ignored("a/x.tmp", &rules));
        assert!(!is_ignored("keep.tmp", &rules));
        // 后规则重新忽略（最后命中者生效）
        let rules2 = vec![
            "*.tmp".to_string(),
            "!keep.tmp".to_string(),
            "keep.tmp".to_string(),
        ];
        assert!(is_ignored("keep.tmp", &rules2));
    }

    #[test]
    fn test_is_ignored_wildcard_ext() {
        let rules = vec!["*.blend".to_string()];
        assert!(is_ignored("models/x.blend", &rules));
        assert!(!is_ignored("models/x.blend1", &rules));
        assert!(!is_ignored("models/x.blenda", &rules));
        // `?` 恰好一位：`yeki.blend` 不被 `*.blend?` 匹配（主文件可跟踪）
        let rules2 = vec!["*.blend?".to_string()];
        assert!(is_ignored("models/x.blend1", &rules2));
        assert!(!is_ignored("models/yeki.blend", &rules2));
    }

    #[test]
    fn test_is_ignored_preset_unaffected() {
        // 应用层预设 `.keinestamp/`（目录规则）——新引擎下行为一致
        let rules = vec![".keinestamp/".to_string()];
        assert!(is_ignored(".keinestamp/web/timeline.json", &rules));
        assert!(!is_ignored("models/x.blend", &rules));
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
