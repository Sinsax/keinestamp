//! 项目级标签组配置。
//!
//! 默认提供建模/绘画/视频三大流程；项目可在 `.keinestamp/tags.toml` 中覆盖。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagGroup {
    pub id: String,
    pub label: String,
    pub tags: Vec<String>,
}

pub fn default_tag_groups() -> Vec<TagGroup> {
    vec![
        TagGroup {
            id: "modeling".to_string(),
            label: "建模流程".to_string(),
            tags: vec![
                "建模".to_string(),
                "UV拆分".to_string(),
                "绑定".to_string(),
                "贴图".to_string(),
                "修复".to_string(),
            ],
        },
        TagGroup {
            id: "painting".to_string(),
            label: "绘画流程".to_string(),
            tags: vec![
                "草图".to_string(),
                "线稿".to_string(),
                "上色".to_string(),
                "细化".to_string(),
                "完稿".to_string(),
            ],
        },
        TagGroup {
            id: "video".to_string(),
            label: "视频流程".to_string(),
            tags: vec![
                "剪辑".to_string(),
                "调色".to_string(),
                "特效".to_string(),
                "字幕".to_string(),
                "导出".to_string(),
            ],
        },
    ]
}

pub fn tag_groups_path(project_root: &Path) -> PathBuf {
    project_root.join(".keinestamp").join("tags.toml")
}

/// 读取项目标签组；文件不存在时返回默认三大流程。
pub fn load_tag_groups(project_root: &Path) -> Result<Vec<TagGroup>> {
    let path = tag_groups_path(project_root);
    if !path.exists() {
        return Ok(default_tag_groups());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read tag groups {:?}", path))?;
    let groups: Vec<TagGroup> = toml::from_str(&content)
        .with_context(|| format!("failed to parse tag groups {:?}", path))?;
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_has_three_flows() {
        let groups = default_tag_groups();
        assert_eq!(groups.len(), 3);
        assert!(groups.iter().any(|g| g.id == "modeling"));
        assert!(groups.iter().any(|g| g.id == "painting"));
        assert!(groups.iter().any(|g| g.id == "video"));
    }

    #[test]
    fn test_load_missing_returns_default() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_tags_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load_tag_groups(&dir)?, default_tag_groups());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
