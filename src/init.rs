use crate::config::{self, MetaConfig};
use crate::detect;
use anyhow::{Context, Result};
use std::path::Path;

pub fn init_project(project_root: &Path) -> Result<()> {
    let project_type = detect::detect_project(project_root)?;

    match project_type {
        detect::ProjectType::Initialized(mut meta) => {
            // 补建所有初始化预设文件（re-init fills gaps）：
            // .lore 缺失（手动删除/旧工程）→ 离线纯本地建仓（不连服务器）
            if !crate::lore_backend::repo_exists(project_root) {
                crate::lore_backend::create_repo(project_root)
                    .with_context(|| format!("failed to create lore repository at {:?}", project_root))?;
            }
            // web/ 结构 + timeline.json + index.html 缺失补建
            ensure_web_structure(&meta.web_root)?;
            // .loreignore 预设（.keinestamp/.keinestamp.toml 隐藏；不覆盖用户规则）
            ensure_loreignore(project_root)?;
            if meta.project_id.is_none() {
                meta.project_id = Some(config::generate_project_id());
                config::write_meta(&config::meta_path(project_root), &meta)?;
            }
            eprintln!("Project already initialized at {:?}", project_root);
            return Ok(());
        }
        detect::ProjectType::Fresh | detect::ProjectType::Existing => {
            perform_init(project_root)?;
        }
    }

    Ok(())
}

fn perform_init(project_root: &Path) -> Result<()> {
    std::fs::create_dir_all(project_root)
        .with_context(|| format!("failed to create project directory {:?}", project_root))?;

    // 创建 Lore 仓库（已存在则跳过；Lore 空提交会被拒，故不做初始提交，
    // 时间线从首次用户 record 开始）
    if !crate::lore_backend::repo_exists(project_root) {
        crate::lore_backend::create_repo(project_root)
            .with_context(|| format!("failed to create lore repository at {:?}", project_root))?;
    }

    // .loreignore 预设（.keinestamp/.keinestamp.toml 隐藏；不覆盖用户规则）
    ensure_loreignore(project_root)?;

    let meta = MetaConfig::new(project_root.to_path_buf());
    let web_root = &meta.web_root;

    std::fs::create_dir_all(web_root)
        .with_context(|| format!("failed to create web directory {:?}", web_root))?;

    write_timeline_json(web_root)?;
    write_index_html(web_root)?;

    let meta_path = config::meta_path(project_root);
    config::write_meta(&meta_path, &meta)
        .with_context(|| format!("failed to write meta config to {:?}", meta_path))?;

    eprintln!(
        "Keinestamp initialized at {:?}",
        project_root
    );

    Ok(())
}

/// .loreignore 预设：隐藏 `.keinestamp/.keinestamp.toml`（项目配置）与
/// `.keinestamp/web/timeline.json`（可重建展示缓存，不入库——入库后每次 rebuild 产生
/// lore 原生 dirty，会导致 branch reset 回退失败）。
/// 不覆盖用户已有规则（只补缺失行）。文件被提交（规则随仓库历史，与 gitignore 同语义）。
fn ensure_loreignore(project_root: &Path) -> Result<()> {
    let mut rules = crate::hidden::load_ignore_rules(project_root);
    const PRESETS: [&str; 2] = [
        ".keinestamp/.keinestamp.toml",
        ".keinestamp/web/timeline.json",
    ];
    for preset in PRESETS {
        if !rules.iter().any(|r| r == preset) {
            rules.insert(0, preset.to_string());
        }
    }
    crate::hidden::save_ignore_rules(project_root, &rules)?;
    Ok(())
}

fn ensure_web_structure(web_root: &Path) -> Result<()> {    if !web_root.exists() {
        std::fs::create_dir_all(web_root)?;
    }

    let timeline_path = web_root.join("timeline.json");
    if !timeline_path.exists() {
        write_timeline_json(web_root)?;
    }

    let index_path = web_root.join("index.html");
    if !index_path.exists() {
        write_index_html(web_root)?;
    }

    Ok(())
}

fn write_timeline_json(web_root: &Path) -> Result<()> {
    let timeline = serde_json::json!({
        "project": "",
        "entries": []
    });
    let content = serde_json::to_string_pretty(&timeline)?;
    std::fs::write(web_root.join("timeline.json"), &content)
        .with_context(|| format!("failed to write timeline.json to {:?}", web_root))?;
    Ok(())
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Keinestamp</title>
</head>
<body>
<p>Keinestamp - 进度追踪</p>
</body>
</html>
"#;

fn write_index_html(web_root: &Path) -> Result<()> {
    std::fs::write(web_root.join("index.html"), INDEX_HTML)
        .with_context(|| format!("failed to write index.html to {:?}", web_root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_init_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_init_fresh() -> Result<()> {
        let dir = tmp_dir("fresh");
        std::fs::remove_dir_all(&dir)?;
        std::fs::create_dir_all(&dir)?;

        init_project(&dir)?;

        assert!(config::meta_path(&dir).exists());
        assert!(dir.join(".lore").exists(), "init 应创建 lore 仓库");
        assert!(crate::lore_backend::repo_exists(&dir));
        assert!(!dir.join(".gitattributes").exists(), "不应生成 .gitattributes（无 LFS）");
        assert!(!dir.join(".git").exists(), "不应创建 git 仓库");
        assert!(dir.join(".keinestamp").join("web").exists());
        assert!(!dir.join("web/.git").exists(), "web 产物并入主仓库，不应有独立 git 仓库");
        assert!(dir.join(".keinestamp").join("web").join("timeline.json").exists());
        assert!(dir.join(".keinestamp").join("web").join("index.html").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_existing_with_blend() -> Result<()> {
        let dir = tmp_dir("existing_with_blend");
        std::fs::write(dir.join("test.blend"), b"")?;

        init_project(&dir)?;

        assert!(config::meta_path(&dir).exists());
        assert!(dir.join(".lore").exists());
        assert!(dir.join(".keinestamp").join("web").exists());
        assert!(!dir.join("web/.git").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_repeat_is_idempotent() -> Result<()> {
        let dir = tmp_dir("repeat");

        init_project(&dir)?;
        let meta_after_first = config::read_meta(&config::meta_path(&dir))?;

        let index_before = std::fs::read_to_string(dir.join(".keinestamp").join("web").join("index.html"))?;

        init_project(&dir)?;

        let meta_after_second = config::read_meta(&config::meta_path(&dir))?;
        assert_eq!(meta_after_first.init_time, meta_after_second.init_time);

        let index_after = std::fs::read_to_string(dir.join(".keinestamp").join("web").join("index.html"))?;
        assert_eq!(index_before, index_after);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_ensure_web_structure_repairs() -> Result<()> {
        let dir = tmp_dir("ensure_web");

        init_project(&dir)?;

        let index_path = dir.join(".keinestamp").join("web").join("index.html");
        std::fs::remove_file(&index_path)?;
        assert!(!index_path.exists());

        let meta = config::read_meta(&config::meta_path(&dir))?;
        ensure_web_structure(&meta.web_root)?;

        assert!(index_path.exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
