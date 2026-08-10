use crate::config::{self, MetaConfig};
use crate::detect;
use anyhow::{Context, Result};
use std::path::Path;

pub fn init_project(project_root: &Path) -> Result<()> {
    let project_type = detect::detect_project(project_root)?;

    match project_type {
        detect::ProjectType::Initialized(mut meta) => {
            ensure_web_structure(&meta.web_root)?;
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

    // 隐藏列表初始化（web/ 与 .keinestamp.toml 预设由 save_hidden 强制保留，
    // 不会覆盖用户已有自定义隐藏项）
    let hidden = crate::hidden::load_hidden(project_root);
    crate::hidden::save_hidden(project_root, &hidden)?;

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

fn ensure_web_structure(web_root: &Path) -> Result<()> {
    if !web_root.exists() {
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

    fn require_server() {
        if !crate::lore_backend::server_available() {
            panic!("loreserver 未运行：请先启动 loreserver.exe（demo 模式）");
        }
    }

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_init_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_init_fresh() -> Result<()> {
        require_server();
        let dir = tmp_dir("fresh");
        std::fs::remove_dir_all(&dir)?;
        std::fs::create_dir_all(&dir)?;

        init_project(&dir)?;

        assert!(config::meta_path(&dir).exists());
        assert!(dir.join(".lore").exists(), "init 应创建 lore 仓库");
        assert!(crate::lore_backend::repo_exists(&dir));
        assert!(!dir.join(".gitattributes").exists(), "不应生成 .gitattributes（无 LFS）");
        assert!(!dir.join(".git").exists(), "不应创建 git 仓库");
        assert!(dir.join("web").exists());
        assert!(!dir.join("web/.git").exists(), "web 产物并入主仓库，不应有独立 git 仓库");
        assert!(dir.join("web").join("timeline.json").exists());
        assert!(dir.join("web").join("index.html").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_existing_with_blend() -> Result<()> {
        require_server();
        let dir = tmp_dir("existing_with_blend");
        std::fs::write(dir.join("test.blend"), b"")?;

        init_project(&dir)?;

        assert!(config::meta_path(&dir).exists());
        assert!(dir.join(".lore").exists());
        assert!(dir.join("web").exists());
        assert!(!dir.join("web/.git").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_repeat_is_idempotent() -> Result<()> {
        require_server();
        let dir = tmp_dir("repeat");

        init_project(&dir)?;
        let meta_after_first = config::read_meta(&config::meta_path(&dir))?;

        let index_before = std::fs::read_to_string(dir.join("web").join("index.html"))?;

        init_project(&dir)?;

        let meta_after_second = config::read_meta(&config::meta_path(&dir))?;
        assert_eq!(meta_after_first.init_time, meta_after_second.init_time);

        let index_after = std::fs::read_to_string(dir.join("web").join("index.html"))?;
        assert_eq!(index_before, index_after);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_ensure_web_structure_repairs() -> Result<()> {
        require_server();
        let dir = tmp_dir("ensure_web");

        init_project(&dir)?;

        let index_path = dir.join("web").join("index.html");
        std::fs::remove_file(&index_path)?;
        assert!(!index_path.exists());

        let meta = config::read_meta(&config::meta_path(&dir))?;
        ensure_web_structure(&meta.web_root)?;

        assert!(index_path.exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
