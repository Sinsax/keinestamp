use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub project: String,
    pub entries: Vec<TimelineEntry>,
    #[serde(default)]
    pub rollback_marker: Option<RollbackMarker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollbackMarker {
    /// 回退点（branch reset 到的 revision sig）
    pub rollback_point: String,
    /// 回退前 HEAD revision sig（撤销回退目标）
    pub head_before: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub id: String,
    pub timestamp: String,
    pub author: String,
    pub message: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub file_status: Vec<String>,
    pub preview: String,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub milestone: bool,
}

pub fn read_timeline(path: &Path) -> Result<Timeline> {
    eprintln!("[debug] read_timeline: {:?}", path);
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read timeline {:?}", path))?;
    let timeline: Timeline = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse timeline {:?}", path))?;
    eprintln!("[debug] read_timeline: {} entries", timeline.entries.len());
    Ok(timeline)
}

pub fn write_timeline(path: &Path, timeline: &Timeline) -> Result<()> {
    eprintln!("[debug] write_timeline: {:?}, {} entries", path, timeline.entries.len());
    let content = serde_json::to_string_pretty(timeline)?;

    let parent = path.parent().context("timeline path has no parent")?;
    std::fs::create_dir_all(parent)?;

    let file_name = path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
    let safe_name = file_name.strip_suffix(".json").unwrap_or(&file_name);
    let tmp_path = parent.join(format!(
        "timeline.tmp.{}.{}.json",
        std::process::id(),
        safe_name,
    ));
    std::fs::write(&tmp_path, &content)
        .with_context(|| format!("failed to write temporary timeline to {:?}", tmp_path))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to rename {:?} to {:?}", tmp_path, path))?;

    Ok(())
}

pub fn append_entry(path: &Path, entry: TimelineEntry) -> Result<()> {
    eprintln!("[debug] append_entry: {:?}, id={}", path, &entry.id[..7.min(entry.id.len())]);
    let mut timeline = if path.exists() {
        read_timeline(path)?
    } else {
        Timeline {
            project: String::new(),
            entries: Vec::new(),
        rollback_marker: None,
        }
    };

    timeline.entries.push(entry);
    write_timeline(path, &timeline)
}

pub fn set_entry_milestone(path: &Path, entry_id: &str, milestone: bool) -> Result<()> {
    eprintln!("[debug] set_entry_milestone: {:?}, id={}, milestone={}", path, entry_id, milestone);
    let mut timeline = read_timeline(path)?;
    let entry = timeline
        .entries
        .iter_mut()
        .find(|e| e.id == entry_id || e.id.starts_with(entry_id))
        .context("entry not found")?;
    entry.milestone = milestone;
    write_timeline(path, &timeline)
}

pub fn remove_entry(path: &Path, entry_id: &str) -> Result<TimelineEntry> {
    eprintln!("[debug] remove_entry: {:?}, id={}", path, entry_id);
    let mut timeline = read_timeline(path)?;
    let pos = timeline
        .entries
        .iter()
        .position(|e| e.id == entry_id || e.id.starts_with(entry_id));
    match pos {
        Some(idx) => {
            let entry = timeline.entries.remove(idx);
            write_timeline(path, &timeline)?;
            Ok(entry)
        }
        None => anyhow::bail!("entry not found: {}", entry_id),
    }
}

pub fn remove_entries_after(path: &Path, entry_id: &str) -> Result<()> {
    eprintln!("[debug] remove_entries_after: {:?}, id={}", path, entry_id);
    let mut timeline = read_timeline(path)?;
    let start = match timeline.entries.iter().position(|e| e.id == entry_id || e.id.starts_with(entry_id)) {
        Some(pos) => pos,
        None => 0,
    };
    if start < timeline.entries.len() {
        timeline.entries.truncate(start);
        write_timeline(path, &timeline)?;
    }
    Ok(())
}

pub fn get_entry<'a>(timeline: &'a Timeline, id: &str) -> Option<&'a TimelineEntry> {
    timeline
        .entries
        .iter()
        .find(|e| e.id == id || e.id.starts_with(id))
}

pub fn timeline_cache_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(crate::hidden::WEB_DIR).join("timeline.json")
}

pub fn read_marker(project_root: &Path) -> Result<Option<RollbackMarker>> {
    let path = timeline_cache_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    Ok(read_timeline(&path)?.rollback_marker)
}

pub fn write_marker(project_root: &Path, marker: &RollbackMarker) -> Result<()> {
    let path = timeline_cache_path(project_root);
    let mut timeline = if path.exists() {
        read_timeline(&path)?
    } else {
        Timeline { project: String::new(), entries: Vec::new(), rollback_marker: None }
    };
    timeline.rollback_marker = Some(marker.clone());
    write_timeline(&path, &timeline)
}

pub fn clear_rollback(project_root: &Path) -> Result<()> {
    let path = timeline_cache_path(project_root);
    let mut timeline = read_timeline(&path)?;
    timeline.rollback_marker = None;
    write_timeline(&path, &timeline)
}

/// 从 revision history 重建展示缓存（web/timeline.json）。
/// 时间戳 = lore revision 的 Date（RFC2822 原样存）；files = 该 revision 相对其父的改动文件。
/// 首条 revision（无父）files 取 dump 全量。任一解析失败 files 为空数组，不中断重建。
pub fn rebuild_cache(project_root: &Path) -> Result<()> {
    let web = project_root.join(crate::hidden::WEB_DIR);
    let timeline_path = web.join("timeline.json");
    let rollback_marker = if timeline_path.exists() {
        read_timeline(&timeline_path).ok().and_then(|t| t.rollback_marker)
    } else {
        None
    };
    let hist = crate::lore_backend::revision_history(project_root)?;
    let mut entries: Vec<TimelineEntry> = hist
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
        .map(|r| TimelineEntry {
            id: r.id.clone(),
            timestamp: lore_date_to_iso(&r.date),
            author: "lore".to_string(),
            message: r.message.clone(),
            files: entry_files(project_root, &r.id),
            file_status: Vec::new(),
            preview: String::new(),
            tag: r.metadata.get("tag").cloned(),
            milestone: r.metadata.get("milestone").map(String::as_str) == Some("true"),
        })
        .collect();
    // v1 语义：entries 旧→新（最新在尾部，append 顺序）；revision_history 是最新在前
    entries.reverse();
    let timeline = Timeline {
        project: project_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        entries,
        rollback_marker,
    };
    write_timeline(&timeline_path, &timeline)
}

/// Lore revision date（RFC2822，如 `Mon, 10 Aug 2026 11:57:09 +0000`）→ ISO 8601（v1 格式，web 模板与前端按 ISO 解析）。
/// 解析失败时原样返回。
fn lore_date_to_iso(date: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(date)
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|_| date.to_string())
}

/// 该 revision 相对其父的改动文件列表（`revision diff <parent> --target <sig>` 状态行解析）。
/// 首条 revision（无父）用 `repository dump` 列全部文件。容错：失败返回空数组。
fn entry_files(project_root: &Path, revision: &str) -> Vec<String> {
    let parent = crate::lore_backend::revision_parent(project_root, revision).unwrap_or(None);
    let output = match &parent {
        Some(p) => crate::lore_backend::run_lore(
            project_root,
            &["revision", "diff", p, "--target", revision],
        ),
        None => crate::lore_backend::run_lore(
            project_root,
            &["repository", "dump", "--revision", revision],
        ),
    };
    match (output, parent) {
        (Ok(out), Some(_)) => parse_diff_paths(&out),
        (Ok(out), None) => parse_dump_paths(&out),
        _ => Vec::new(),
    }
}

fn parse_diff_paths(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let t = l.trim();
            let mut chars = t.chars();
            let status = chars.next()?;
            if !matches!(status, 'A' | 'M' | 'D') {
                return None;
            }
            let rest = t[status.len_utf8()..].trim();
            if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            }
        })
        .collect()
}

fn parse_dump_paths(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            if !l.contains(" addr ") {
                return None;
            }
            let path = l.split_whitespace().next()?;
            if path.ends_with('/') {
                None
            } else {
                Some(path.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("keinestamp_timeline_{}.json", name));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn make_entry(id: &str) -> TimelineEntry {
        TimelineEntry {
            id: id.to_string(),
            timestamp: "2026-06-13T12:00:00Z".to_string(),
            author: "test".to_string(),
            message: "test commit".to_string(),
            files: vec!["test.blend".to_string()],
            file_status: vec!["A".to_string()],
            preview: format!("previews/{}.png", id),
            tag: None,
            milestone: false,
        }
    }

    #[test]
    fn test_set_entry_milestone() -> Result<()> {
        let path = tmp_path("milestone");
        let mut timeline = Timeline {
            project: "P".to_string(),
            entries: vec![make_entry("abc123def456")],
        rollback_marker: None,
        };
        write_timeline(&path, &timeline)?;

        set_entry_milestone(&path, "abc123def456", true)?;
        let read = read_timeline(&path)?;
        assert!(read.entries[0].milestone);

        set_entry_milestone(&path, "abc123", false)?;
        let read = read_timeline(&path)?;
        assert!(!read.entries[0].milestone);

        assert!(set_entry_milestone(&path, "nope", true).is_err());
        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_old_timeline_without_milestone_parses() -> Result<()> {
        let path = tmp_path("old_milestone");
        let old_json = r#"{"project":"P","entries":[{"id":"abc","timestamp":"2026-06-13T12:00:00Z","author":"a","message":"m","files":["f.blend"],"preview":"p"}]}"#;
        std::fs::write(&path, old_json)?;
        let read = read_timeline(&path)?;
        assert!(!read.entries[0].milestone);
        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_write_and_read() -> Result<()> {
        let path = tmp_path("write_read");
        let mut timeline = Timeline {
            project: "Test".to_string(),
            entries: vec![],
        rollback_marker: None,
        };
        timeline.entries.push(make_entry("abc123"));

        write_timeline(&path, &timeline)?;
        let read_back = read_timeline(&path)?;

        assert_eq!(read_back.project, "Test");
        assert_eq!(read_back.entries.len(), 1);
        assert_eq!(read_back.entries[0].id, "abc123");

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_append_entry() -> Result<()> {
        let path = tmp_path("append");

        append_entry(&path, make_entry("first"))?;
        let t = read_timeline(&path)?;
        assert_eq!(t.entries.len(), 1);

        append_entry(&path, make_entry("second"))?;
        let t = read_timeline(&path)?;
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[0].id, "first");
        assert_eq!(t.entries[1].id, "second");

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_get_entry() -> Result<()> {
        let path = tmp_path("get");
        append_entry(&path, make_entry("hash123"))?;
        let t = read_timeline(&path)?;

        let found = get_entry(&t, "hash123");
        assert!(found.is_some());
        assert_eq!(found.unwrap().author, "test");

        let not_found = get_entry(&t, "nonexistent");
        assert!(not_found.is_none());

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_remove_entry_by_full_hash() -> Result<()> {
        let path = tmp_path("remove");
        append_entry(&path, make_entry("abc123def456"))?;
        append_entry(&path, make_entry("789012ghi345"))?;

        let removed = remove_entry(&path, "abc123def456")?;
        assert_eq!(removed.id, "abc123def456");

        let t = read_timeline(&path)?;
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].id, "789012ghi345");

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_remove_entry_by_prefix() -> Result<()> {
        let path = tmp_path("remove_prefix");
        append_entry(&path, make_entry("abc123def456"))?;
        append_entry(&path, make_entry("789012ghi345"))?;

        let removed = remove_entry(&path, "abc")?;
        assert_eq!(removed.id, "abc123def456");

        let t = read_timeline(&path)?;
        assert_eq!(t.entries.len(), 1);

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_remove_entry_not_found() -> Result<()> {
        let path = tmp_path("remove_miss");
        append_entry(&path, make_entry("abc"))?;

        let result = remove_entry(&path, "nonexistent");
        assert!(result.is_err());

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_remove_entry_updates_preview_path() -> Result<()> {
        let path = tmp_path("remove_preview");
        let mut entry = make_entry("first");
        entry.preview = "previews/first.png".to_string();
        append_entry(&path, entry)?;
        append_entry(&path, make_entry("second"))?;

        let removed = remove_entry(&path, "first")?;
        assert_eq!(removed.preview, "previews/first.png");

        let t = read_timeline(&path)?;
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].id, "second");

        std::fs::remove_file(&path)?;
        Ok(())
    }

    fn require_server() {
        if !crate::lore_backend::server_available() {
            panic!("loreserver 未运行：请先启动 loreserver.exe（demo 模式）");
        }
    }

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_tl_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_rebuild_cache_from_lore_history() -> Result<()> {
        require_server();
        let root = tmp_root("rebuild");
        crate::lore_backend::create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::write(root.join("b.txt"), "2").unwrap();
        crate::lore_project::record(&root, "first", None, None, &vec!["web/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "3").unwrap();
        crate::lore_project::record(&root, "second", None, None, &vec!["web/".to_string()]).unwrap();

        rebuild_cache(&root)?;
        let timeline = read_timeline(&timeline_cache_path(&root))?;
        assert_eq!(timeline.entries.len(), 2);
        // entries 旧→新（v1 语义，最新在尾部）
        assert_eq!(timeline.entries[0].message, "first");
        assert_eq!(timeline.entries[1].message, "second");
        assert!(!timeline.entries[1].files.is_empty());
        assert!(!timeline.entries[1].timestamp.is_empty());
        // 时间戳应为 ISO 8601（含 T，可被 split('T')/slice(0,10) 解析）
        assert!(timeline.entries[1].timestamp.contains('T'));
        Ok(())
    }
}
