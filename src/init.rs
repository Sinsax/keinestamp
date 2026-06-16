use crate::config::{self, MetaConfig};
use crate::detect;
use anyhow::{Context, Result};
use std::path::Path;

pub fn init_project(project_root: &Path) -> Result<()> {
    let project_type = detect::detect_project(project_root)?;

    match project_type {
        detect::ProjectType::Initialized(mut meta) => {
            ensure_project_gitignore(project_root)?;
            ensure_project_gitattributes(project_root)?;
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

    let project_git = project_root.join(".git");
    if !project_git.exists() {
        git_init(project_root, "project")?;
    } else {
        eprintln!("Project git repository already exists, skipping init");
    }

    ensure_project_gitignore(project_root)?;
    ensure_project_gitattributes(project_root)?;

    let meta = MetaConfig::new(project_root.to_path_buf());
    let web_root = &meta.web_root;

    std::fs::create_dir_all(web_root)
        .with_context(|| format!("failed to create web directory {:?}", web_root))?;

    let web_git = web_root.join(".git");
    if !web_git.exists() {
        git_init(web_root, "web")?;
    } else {
        eprintln!("Web git repository already exists, skipping init");
    }

    write_timeline_json(web_root)?;
    write_index_html(web_root)?;

    let meta_path = config::meta_path(project_root);
    config::write_meta(&meta_path, &meta)
        .with_context(|| format!("failed to write meta config to {:?}", meta_path))?;

    eprintln!(
        "SakiStep initialized at {:?}",
        project_root
    );

    Ok(())
}

fn ensure_web_structure(web_root: &Path) -> Result<()> {
    if !web_root.exists() {
        std::fs::create_dir_all(web_root)?;
    }

    let web_git = web_root.join(".git");
    if !web_git.exists() {
        git_init(web_root, "web")?;
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

const GITATTRIBUTES: &str = r#"* text=auto eol=lf

# 3D 文件
*.blend filter=lfs diff=lfs merge=lfs -text
*.fbx filter=lfs diff=lfs merge=lfs -text
*.obj filter=lfs diff=lfs merge=lfs -text
*.glb filter=lfs diff=lfs merge=lfs -text
*.gltf filter=lfs diff=lfs merge=lfs -text
*.mtl filter=lfs diff=lfs merge=lfs -text
*.pmx filter=lfs diff=lfs merge=lfs -text
*.vrm filter=lfs diff=lfs merge=lfs -text
*.warudo filter=lfs diff=lfs merge=lfs -text
*.vroid filter=lfs diff=lfs merge=lfs -text

# sp贴图工程
*.spp filter=lfs diff=lfs merge=lfs -text

# 图片文件
*.png filter=lfs diff=lfs merge=lfs -text
*.psd filter=lfs diff=lfs merge=lfs -text
*.jpg filter=lfs diff=lfs merge=lfs -text
*.jpeg filter=lfs diff=lfs merge=lfs -text
*.tga filter=lfs diff=lfs merge=lfs -text
*.bmp filter=lfs diff=lfs merge=lfs -text
*.gif filter=lfs diff=lfs merge=lfs -text
*.tiff filter=lfs diff=lfs merge=lfs -text
*.exr filter=lfs diff=lfs merge=lfs -text
*.dds filter=lfs diff=lfs merge=lfs -text
*.hdr filter=lfs diff=lfs merge=lfs -text
*.svg filter=lfs diff=lfs merge=lfs -text
*.pdf filter=lfs diff=lfs merge=lfs -text
*.spa filter=lfs diff=lfs merge=lfs -text

# 视频文件
*.mp4 filter=lfs diff=lfs merge=lfs -text
*.mov filter=lfs diff=lfs merge=lfs -text
*.avi filter=lfs diff=lfs merge=lfs -text
*.webm filter=lfs diff=lfs merge=lfs -text
*.mkv filter=lfs diff=lfs merge=lfs -text

# 音频文件
*.wav filter=lfs diff=lfs merge=lfs -text
*.mp3 filter=lfs diff=lfs merge=lfs -text
*.ogg filter=lfs diff=lfs merge=lfs -text
*.m4a filter=lfs diff=lfs merge=lfs -text
*.flac filter=lfs diff=lfs merge=lfs -text

# 压缩包
*.zip filter=lfs diff=lfs merge=lfs -text
*.rar filter=lfs diff=lfs merge=lfs -text
*.7z filter=lfs diff=lfs merge=lfs -text
*.tar filter=lfs diff=lfs merge=lfs -text

# Unity包
*.unitypackage filter=lfs diff=lfs merge=lfs -text

# 动画文件
*.vmd filter=lfs diff=lfs merge=lfs -text
"#;

fn ensure_project_gitattributes(project_root: &Path) -> Result<()> {
    let gitattributes_path = project_root.join(".gitattributes");
    if !gitattributes_path.exists() {
        std::fs::write(&gitattributes_path, GITATTRIBUTES)
            .with_context(|| format!("failed to write {:?}", gitattributes_path))?;
    }
    Ok(())
}

fn ensure_project_gitignore(project_root: &Path) -> Result<()> {
    let gitignore_path = project_root.join(".gitignore");
    let mut content = String::new();
    if gitignore_path.exists() {
        content = std::fs::read_to_string(&gitignore_path)?;
    }
    for pattern in &["web-timeline/", ".saki-step.toml"] {
        if !content.contains(pattern) {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(pattern);
            content.push('\n');
        }
    }
    if !gitignore_path.exists() || content != std::fs::read_to_string(&gitignore_path).unwrap_or_default() {
        std::fs::write(&gitignore_path, &content)?;
    }
    Ok(())
}

fn git_init(path: &Path, label: &str) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .with_context(|| format!("failed to execute git init for {} at {:?}", label, path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git init for {} failed: {}", label, stderr);
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
<title>SakiStep</title>
</head>
<body>
<p>SakiStep - 进度追踪</p>
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
        let dir = std::env::temp_dir().join(format!("saki_init_{}", name));
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
        assert!(dir.join(".git").exists());
        assert!(dir.join("web-timeline").exists());
        assert!(dir.join("web-timeline").join(".git").exists());
        assert!(dir.join("web-timeline").join("timeline.json").exists());
        assert!(dir.join("web-timeline").join("index.html").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_existing_with_blend() -> Result<()> {
        let dir = tmp_dir("existing_with_blend");
        std::fs::write(dir.join("test.blend"), b"")?;

        init_project(&dir)?;

        assert!(config::meta_path(&dir).exists());
        assert!(dir.join(".git").exists());
        assert!(dir.join("web-timeline").exists());
        assert!(dir.join("web-timeline").join(".git").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_existing_with_git() -> Result<()> {
        let dir = tmp_dir("existing_with_git");
        std::fs::write(dir.join("test.blend"), b"")?;

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()?;

        let git_dir_before = dir.join(".git");
        assert!(git_dir_before.exists());

        init_project(&dir)?;

        assert!(git_dir_before.exists());
        assert!(config::meta_path(&dir).exists());
        assert!(dir.join("web-timeline").exists());
        assert!(dir.join("web-timeline").join(".git").exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_init_repeat_is_idempotent() -> Result<()> {
        let dir = tmp_dir("repeat");

        init_project(&dir)?;
        let meta_after_first = config::read_meta(&config::meta_path(&dir))?;

        let index_before = std::fs::read_to_string(dir.join("web-timeline").join("index.html"))?;

        init_project(&dir)?;

        let meta_after_second = config::read_meta(&config::meta_path(&dir))?;
        assert_eq!(meta_after_first.init_time, meta_after_second.init_time);

        let index_after = std::fs::read_to_string(dir.join("web-timeline").join("index.html"))?;
        assert_eq!(index_before, index_after);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_ensure_web_structure_repairs() -> Result<()> {
        let dir = tmp_dir("ensure_web");

        init_project(&dir)?;

        let index_path = dir.join("web-timeline").join("index.html");
        std::fs::remove_file(&index_path)?;
        assert!(!index_path.exists());

        let meta = config::read_meta(&config::meta_path(&dir))?;
        ensure_web_structure(&meta.web_root)?;

        assert!(index_path.exists());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
