//! 项目级流程与标签配置。
//!
//! 默认只提供一个 Blender 流程；项目可在 `.keinestamp/tags.toml` 中自定义流程与标签
//! （删除该文件即恢复默认 Blender 流程）。配置文件为程序私有产物，被 `.loreignore` 整体排除、不入库。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagGroup {
    pub id: String,
    pub label: String,
    pub tags: Vec<String>,
}

/// 配置文件首行注释（人工可读；解析 TOML 时注释会被忽略）。
const TAG_GROUPS_HEADER: &str =
    "# Keinestamp 流程与标签（项目级配置，不入库；删除本文件即恢复默认 Blender 流程）";

/// tags.toml 的包装结构。
///
/// TOML 根必须是表（table），不能直接是数组，所以流程列表放进 `[[flows]]` 数组表。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TagGroupsFile {
    #[serde(default)]
    flows: Vec<TagGroup>,
}

/// 默认流程：仅 Blender。
pub fn default_tag_groups() -> Vec<TagGroup> {
    vec![TagGroup {
        id: "blender".to_string(),
        label: "Blender".to_string(),
        tags: vec![
            "建模".to_string(),
            "UV拆分".to_string(),
            "绑定".to_string(),
            "贴图".to_string(),
            "修复".to_string(),
        ],
    }]
}

pub fn tag_groups_path(project_root: &Path) -> PathBuf {
    project_root.join(".keinestamp").join("tags.toml")
}

/// 规范化流程列表：trim、丢弃空 id/label、id 去重（保留首个）、组内标签去重去空。
/// 允许流程的 tags 为空（用户只使用自定义标签）。规范化为空时回落默认 Blender 流程。
pub fn normalize_tag_groups(groups: Vec<TagGroup>) -> Vec<TagGroup> {
    let mut out: Vec<TagGroup> = Vec::with_capacity(groups.len());
    let mut seen_ids: HashSet<String> = HashSet::new();
    for group in groups {
        let id = group.id.trim().to_string();
        let label = group.label.trim().to_string();
        if id.is_empty() || label.is_empty() {
            continue;
        }
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let mut tags: Vec<String> = Vec::with_capacity(group.tags.len());
        let mut seen_tags: HashSet<String> = HashSet::new();
        for tag in group.tags {
            let tag = tag.trim().to_string();
            if tag.is_empty() {
                continue;
            }
            if seen_tags.insert(tag.clone()) {
                tags.push(tag);
            }
        }
        out.push(TagGroup { id, label, tags });
    }
    if out.is_empty() {
        default_tag_groups()
    } else {
        out
    }
}

/// 读取项目流程配置。
///
/// - 文件不存在 → 默认流程；
/// - 读取/解析失败或结构不符合预期 → 打印调试信息并降级为默认流程（不让项目加载失败，
///   与 `preview.rs` 的降级思路一致）；
/// - 解析成功 → 规范化后返回。
pub fn load_tag_groups(project_root: &Path) -> Result<Vec<TagGroup>> {
    let path = tag_groups_path(project_root);
    if !path.exists() {
        return Ok(default_tag_groups());
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[debug] 读取流程配置失败（{:?}），降级为默认 Blender 流程: {e}",
                path
            );
            return Ok(default_tag_groups());
        }
    };
    match toml::from_str::<TagGroupsFile>(&content) {
        Ok(file) => Ok(normalize_tag_groups(file.flows)),
        Err(e) => {
            eprintln!(
                "[debug] 解析流程配置失败（{:?}），降级为默认 Blender 流程: {e}",
                path
            );
            Ok(default_tag_groups())
        }
    }
}

/// 规范化后写入项目流程配置，返回实际写入的列表（前端用它刷新 UI）。
pub fn save_tag_groups(project_root: &Path, groups: &[TagGroup]) -> Result<Vec<TagGroup>> {
    let normalized = normalize_tag_groups(groups.to_vec());
    let file = TagGroupsFile {
        flows: normalized.clone(),
    };
    let body = toml::to_string(&file).context("failed to serialize tag groups")?;
    let content = format!("{TAG_GROUPS_HEADER}\n{body}");
    let path = tag_groups_path(project_root);
    crate::process_util::atomic_write(&path, content.as_bytes())
        .with_context(|| format!("failed to write tag groups {:?}", path))?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_SEQ: AtomicUsize = AtomicUsize::new(0);

    /// 每个测试独立临时目录（pid + 自增序号，避免同进程并行测试撞名）。
    fn temp_project(tag: &str) -> PathBuf {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "keinestamp_tags_{}_{}_{}",
            tag,
            std::process::id(),
            seq
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    fn group(id: &str, label: &str, tags: &[&str]) -> TagGroup {
        TagGroup {
            id: id.to_string(),
            label: label.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_default_is_single_blender_flow() {
        let groups = default_tag_groups();
        assert_eq!(groups.len(), 1, "默认只应有一个流程");
        assert_eq!(groups[0].id, "blender");
        assert_eq!(groups[0].label, "Blender");
        assert!(!groups[0].tags.is_empty(), "默认流程标签不应为空");
    }

    #[test]
    fn test_load_missing_returns_default() -> Result<()> {
        let dir = temp_project("missing");
        let groups = load_tag_groups(&dir)?;
        assert_eq!(groups, default_tag_groups());
        cleanup(&dir);
        Ok(())
    }

    #[test]
    fn test_save_load_roundtrip() -> Result<()> {
        let dir = temp_project("roundtrip");
        let input = vec![
            group("blender", "Blender", &["建模", "UV拆分", "绑定"]),
            group("custom", "自定义流程", &["草图", "上色"]),
        ];
        let saved = save_tag_groups(&dir, &input)?;
        assert_eq!(saved, input, "返回值应与输入一致（已规范化）");

        let path = tag_groups_path(&dir);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("[[flows]]"),
            "文件应为 [[flows]] 数组表格式，实际内容：\n{content}"
        );
        assert!(content.contains(TAG_GROUPS_HEADER), "文件应含说明注释");

        let loaded = load_tag_groups(&dir)?;
        assert_eq!(loaded, saved, "save → load 应保持内容一致");
        cleanup(&dir);
        Ok(())
    }

    #[test]
    fn test_normalize_trims_and_drops_invalid() {
        // trim id/label/tags；空 id、空 label 丢弃
        let input = vec![
            group("  blender  ", "  Blender  ", &[" 建模 ", "UV拆分"]),
            group("   ", "空 id", &["x"]),
            group("empty_label", "   ", &["y"]),
        ];
        let out = normalize_tag_groups(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "blender");
        assert_eq!(out[0].label, "Blender");
        assert_eq!(out[0].tags, vec!["建模".to_string(), "UV拆分".to_string()]);
    }

    #[test]
    fn test_normalize_duplicate_ids_keep_first() {
        let input = vec![
            group("blender", "Blender", &["建模"]),
            group("blender", "第二个 Blender", &["贴图"]),
        ];
        let out = normalize_tag_groups(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "Blender", "重复 id 应保留第一个");
        assert_eq!(out[0].tags, vec!["建模".to_string()]);
    }

    #[test]
    fn test_normalize_dedupes_tags_and_allows_empty_tags() {
        let input = vec![
            group(
                "blender",
                "Blender",
                &["建模", "建模", "  ", " 贴图 ", "贴图"],
            ),
            group("empty", "自定义流程", &[]),
        ];
        let out = normalize_tag_groups(input);
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].tags,
            vec!["建模".to_string(), "贴图".to_string()],
            "组内标签去重（保留首个出现顺序）并去空"
        );
        assert!(out[1].tags.is_empty(), "允许 tags 为空的流程");
    }

    #[test]
    fn test_normalize_empty_falls_back_to_default() {
        assert_eq!(
            normalize_tag_groups(Vec::new()),
            default_tag_groups(),
            "规范化为空应回落默认流程"
        );
        let only_invalid = vec![group("", "", &[]), group("x", "  ", &["a"])];
        assert_eq!(normalize_tag_groups(only_invalid), default_tag_groups());
    }

    #[test]
    fn test_load_corrupted_degrades_to_default() -> Result<()> {
        let dir = temp_project("corrupted");
        let path = tag_groups_path(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"this is not toml [[[ ???").unwrap();
        let groups = load_tag_groups(&dir)?;
        assert_eq!(groups, default_tag_groups(), "损坏文件应降级为默认流程");
        cleanup(&dir);
        Ok(())
    }

    #[test]
    fn test_load_wrong_shape_degrades_to_default() -> Result<()> {
        let dir = temp_project("wrongshape");
        let path = tag_groups_path(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // 合法 TOML，但结构不符合 TagGroupsFile（flows 不是数组表）
        std::fs::write(&path, b"flows = 42\n").unwrap();
        let groups = load_tag_groups(&dir)?;
        assert_eq!(groups, default_tag_groups());
        cleanup(&dir);
        Ok(())
    }
}
