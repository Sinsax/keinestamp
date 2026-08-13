use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub project: String,
    pub entries: Vec<TimelineEntry>,
    #[serde(default)]
    pub rollback_marker: Option<RollbackMarker>,
    /// 用户删除的条目 id（delete_entry 记录；rebuild_cache 从历史重建时过滤——
    /// 否则每次记录后重建会把被删条目拉回来）
    #[serde(default)]
    pub removed_entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollbackMarker {
    /// 回退后 HEAD（branch reset 到的 X-1 revision sig，恢复按钮条件 = current == 此值）
    pub rollback_point: String,
    /// 灰显起点（被选中的 X revision sig；X 及之后的时间线记录灰显）
    pub superseded_point: String,
    /// 回退前 HEAD revision sig（撤销回退目标）
    pub head_before: String,
    /// X 相对 X-1 删除的文件（回退时磁盘保留并标记"回退删除"；记录时算删除）
    #[serde(default)]
    pub deleted_files: Vec<String>,
    /// X 相对 X-1 修改/新增的文件（检出为未提交的"回退改动"；撤销回退时默认丢弃，
    /// 冲突弹窗选"当前版本改动"才带回）
    #[serde(default)]
    pub change_files: Vec<String>,
    /// 检出文件内容 hash（rollback 检出后对 change_files 算 sha256）：工作区 hash 与检出
    /// hash 不符 = 用户真修改（标签"已修改"）；一致 = 纯检出（标签"已检出"，不阻碍连续回退）
    #[serde(default)]
    pub change_hashes: std::collections::HashMap<String, String>,
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
    /// 被顶替出主线的记录（回退后显示灰显；数据保留在 store 但不在主线历史）
    #[serde(default)]
    pub superseded: bool,
    /// 父记录签名（主线按顺序推导；回退时读缓存免 lore 调用；旧缓存空 → fallback lore revision info）
    #[serde(default)]
    pub parent: String,
}

pub fn read_timeline(path: &Path) -> Result<Timeline> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read timeline {:?}", path))?;
    let timeline: Timeline = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse timeline {:?}", path))?;
    Ok(timeline)
}

pub fn write_timeline(path: &Path, timeline: &Timeline) -> Result<()> {
    let content = serde_json::to_string_pretty(timeline)?;

    crate::process_util::atomic_write(path, content.as_bytes())
        .with_context(|| format!("failed to write timeline to {:?}", path))?;

    Ok(())
}

pub fn append_entry(path: &Path, entry: TimelineEntry) -> Result<()> {
    let mut timeline = if path.exists() {
        read_timeline(path)?
    } else {
        Timeline {
            project: String::new(),
            entries: Vec::new(),
        rollback_marker: None,
        removed_entries: Vec::new(),
        }
    };

    timeline.entries.push(entry);
    write_timeline(path, &timeline)
}

pub fn set_entry_milestone(path: &Path, entry_id: &str, milestone: bool) -> Result<()> {
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
    let mut timeline = read_timeline(path)?;
    let pos = timeline
        .entries
        .iter()
        .position(|e| e.id == entry_id || e.id.starts_with(entry_id));
    match pos {
        Some(idx) => {
            let entry = timeline.entries.remove(idx);
            // tombstone：记录被删条目（rebuild_cache 重建时过滤，否则条目会被拉回）
            if !timeline.removed_entries.contains(&entry.id) {
                timeline.removed_entries.push(entry.id.clone());
            }
            write_timeline(path, &timeline)?;
            Ok(entry)
        }
        None => anyhow::bail!("entry not found: {}", entry_id),
    }
}

pub fn remove_entries_after(path: &Path, entry_id: &str) -> Result<()> {
    let mut timeline = read_timeline(path)?;
    let start = match timeline.entries.iter().position(|e| e.id == entry_id || e.id.starts_with(entry_id)) {
        Some(pos) => pos,
        // 防御：id 未命中时禁止 truncate(0) 清空整条时间线（数据丢失陷阱）
        None => anyhow::bail!("entry not found: {}", entry_id),
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
        Timeline { project: String::new(), entries: Vec::new(), rollback_marker: None, removed_entries: Vec::new() }
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
    // 旧缓存：保留 rollback_marker + 主线 entries 的 preview（preview 是 web 产物路径，
    // 不在 revision metadata 里——重建时从旧缓存沿用，否则每次记录后旧记录的 3D 标签丢失）
    let old_timeline = if timeline_path.exists() {
        read_timeline(&timeline_path).ok()
    } else {
        None
    };
    let rollback_marker = old_timeline.as_ref().and_then(|t| t.rollback_marker.clone());
    // 用户删除的条目 tombstone：重建时过滤（防止被删条目从历史拉回）
    let removed: std::collections::HashSet<String> = old_timeline
        .as_ref()
        .map(|t| t.removed_entries.iter().cloned().collect())
        .unwrap_or_default();
    let old_previews: std::collections::HashMap<String, String> = old_timeline
        .as_ref()
        .map(|t| {
            t.entries
                .iter()
                .map(|e| (e.id.clone(), e.preview.clone()))
                .collect()
        })
        .unwrap_or_default();
    let hist = crate::lore_backend::revision_history(project_root)?;
    let mut entries: Vec<TimelineEntry> = hist
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("init"))
        .filter(|r| !removed.contains(&r.id))
        .map(|r| TimelineEntry {
            id: r.id.clone(),
            timestamp: lore_date_to_iso(&r.date),
            author: "lore".to_string(),
            message: r.message.clone(),
            files: entry_files(project_root, &r.id),
            file_status: Vec::new(),
            preview: old_previews.get(&r.id).cloned().unwrap_or_default(),
            tag: r.metadata.get("tag").cloned(),
            milestone: r.metadata.get("milestone").map(String::as_str) == Some("true"),
            superseded: false,
            parent: String::new(),
        })
        .collect();
    // v1 语义：entries 旧→新（最新在尾部，append 顺序）；revision_history 是最新在前
    entries.reverse();
    // 主线即父链：旧→新顺序下 entries[i].parent = entries[i-1].id（回退时读缓存免 lore 调用）
    for i in 1..entries.len() {
        entries[i].parent = entries[i - 1].id.clone();
    }
    // 保留被顶替出主线的旧记录（v1 行为：回退后回退点之后的记录仍显示，灰显不可回退）：
    // 旧缓存中 id 不在新主线的条目 → 标记 superseded 并按时间戳插入正确位置（回退点与新记录之间）
    let old_entries = old_timeline.map(|t| t.entries);
    if let Some(old) = old_entries {
        let new_ids: std::collections::HashSet<&str> =
            entries.iter().map(|e| e.id.as_str()).collect();
        let superseded: Vec<TimelineEntry> = old
            .into_iter()
            .filter(|e| !new_ids.contains(e.id.as_str()))
            .map(|mut e| {
                e.superseded = true;
                e
            })
            .collect();
        entries.extend(superseded);
        // ISO 8601 时间戳字典序 = 时间序；被顶替记录插入其真实时间位置（如 D 与 H 之间）
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }
    let timeline = Timeline {
        project: project_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        entries,
        rollback_marker,
        removed_entries: removed.into_iter().collect(),
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
    let parent = match crate::lore_backend::revision_parent(project_root, revision) {
        Ok(p) => p,
        Err(e) => {
            // 读取父失败：不要错误地落入"全树 dump"分支（会把该 revision 的全部文件误列为改动）
            eprintln!("[debug] entry_files: revision_parent 失败（{}），返回空文件列表", e);
            return Vec::new();
        }
    };
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
            // dump 行格式 `<path> id <N> ... addr ...`——路径可能含空格，
            // 必须用 " id " 定位取完整路径（split_whitespace 会截断）
            let sep = l.find(" id ")?;
            let path = l[..sep].trim();
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
            superseded: false,
            parent: String::new(),
        }
    }

    #[test]
    fn test_parse_dump_paths_keeps_spaces() {
        // dump 行格式 `<path> id <N> ... addr ...`——路径含空格必须完整保留
        let out = "refer/芙姆米新1皮三视图 拷贝.png id 12 ... addr 0xabc123\n\
                   refer/ dir-id 3 ... (目录行无 addr)\n\
                   ChatGPT Image 2026年7月24日.png id 5 ... addr 0xdef456\n\
                   a/b/clean.png id 1 ... addr 0x111";
        let paths = parse_dump_paths(out);
        assert_eq!(
            paths,
            vec![
                "refer/芙姆米新1皮三视图 拷贝.png",
                "ChatGPT Image 2026年7月24日.png",
                "a/b/clean.png",
            ]
        );
    }

    #[test]
    fn test_set_entry_milestone() -> Result<()> {
        let path = tmp_path("milestone");
        let mut timeline = Timeline {
            project: "P".to_string(),
            entries: vec![make_entry("abc123def456")],
        rollback_marker: None,
        removed_entries: Vec::new(),
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
        removed_entries: Vec::new(),
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

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_tl_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_rebuild_cache_preserves_old_previews() -> Result<()> {
        let root = tmp_root("rebuildprev");
        crate::lore_backend::create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        crate::lore_project::record(&root, "first", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "2").unwrap();
        crate::lore_project::record(&root, "second", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        rebuild_cache(&root)?;
        // 旧缓存给第一条记录写入 preview（模拟 GLB 渲染产物）
        let path = timeline_cache_path(&root);
        let mut tl = read_timeline(&path)?;
        tl.entries[0].preview = "previews_glb/abc123_prev.glb".to_string();
        write_timeline(&path, &tl)?;
        // 再记录一条 → rebuild → 旧 preview 应保留（不被清空）
        std::fs::write(root.join("a.txt"), "3").unwrap();
        crate::lore_project::record(&root, "third", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let tl2 = read_timeline(&path)?;
        let first = tl2.entries.iter().find(|e| e.message == "first").unwrap();
        assert_eq!(
            first.preview, "previews_glb/abc123_prev.glb",
            "重建后旧记录的 preview 应保留"
        );
        Ok(())
    }

    #[test]
    fn test_rebuild_cache_from_lore_history() -> Result<()> {
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

    #[test]
    fn test_rebuild_keeps_superseded_entries() -> Result<()> {
        let root = tmp_root("rebuildsup");
        crate::lore_backend::create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        crate::lore_project::record(&root, "A", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "2").unwrap();
        crate::lore_project::record(&root, "B", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "3").unwrap();
        crate::lore_project::record(&root, "C", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist = crate::lore_backend::revision_history(&root).unwrap();
        let r2 = hist[1].id.clone(); // B
        crate::lore_project::rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // 顶替：回退状态下记录 D
        std::fs::write(root.join("a.txt"), "4").unwrap();
        crate::lore_project::record(&root, "D", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let timeline = read_timeline(&timeline_cache_path(&root))?;
        // 主线 [A B D] + 被顶替的 C（superseded，v1 行为：仍显示灰显）
        assert_eq!(timeline.entries.len(), 4);
        let c = timeline.entries.iter().find(|e| e.message == "C").unwrap();
        assert!(c.superseded, "被顶替记录应标记 superseded");
        let d = timeline.entries.iter().find(|e| e.message == "D").unwrap();
        assert!(!d.superseded, "主线记录不应 superseded");
        Ok(())
    }
}
