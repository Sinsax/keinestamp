use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub project: String,
    pub entries: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn get_entry<'a>(timeline: &'a Timeline, id: &str) -> Option<&'a TimelineEntry> {
    timeline
        .entries
        .iter()
        .find(|e| e.id == id || e.id.starts_with(id))
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
}
