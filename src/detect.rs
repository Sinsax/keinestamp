use crate::config;
use crate::config::MetaConfig;
use anyhow::Result;
use std::path::Path;

pub enum ProjectType {
    Fresh,
    Existing,
    Initialized(MetaConfig),
}

pub fn detect_project(path: &Path) -> Result<ProjectType> {
    if !path.exists() {
        return Ok(ProjectType::Fresh);
    }
    if !path.is_dir() {
        anyhow::bail!("path is not a directory: {:?}", path);
    }

    let meta_path = config::meta_path(path);
    if meta_path.exists() {
        let meta = config::read_meta(&meta_path)?;
        return Ok(ProjectType::Initialized(meta));
    }

    let has_blend = has_blend_files(path);
    if has_blend {
        Ok(ProjectType::Existing)
    } else {
        Ok(ProjectType::Fresh)
    }
}

fn has_blend_files(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "blend") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_detect_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_detect_fresh_empty_dir() -> Result<()> {
        let dir = tmp_dir("fresh_empty");
        let pt = detect_project(&dir)?;
        assert!(matches!(pt, ProjectType::Fresh));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_detect_fresh_nonexistent() -> Result<()> {
        let dir = Path::new("/tmp/keinestamp_nonexistent_dir_for_test");
        let _ = std::fs::remove_dir_all(dir);
        let pt = detect_project(dir)?;
        assert!(matches!(pt, ProjectType::Fresh));
        Ok(())
    }

    #[test]
    fn test_detect_existing() -> Result<()> {
        let dir = tmp_dir("existing");
        std::fs::write(dir.join("test.blend"), b"")?;
        let pt = detect_project(&dir)?;
        assert!(matches!(pt, ProjectType::Existing));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_detect_existing_no_blend() -> Result<()> {
        let dir = tmp_dir("existing_no_blend");
        std::fs::write(dir.join("test.txt"), b"hello")?;
        let pt = detect_project(&dir)?;
        assert!(matches!(pt, ProjectType::Fresh));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_detect_initialized() -> Result<()> {
        let dir = tmp_dir("initialized");
        let meta = MetaConfig::new(dir.clone());
        let meta_path = config::meta_path(&dir);
        config::write_meta(&meta_path, &meta)?;

        let pt = detect_project(&dir)?;
        assert!(matches!(pt, ProjectType::Initialized(_)));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_detect_path_is_file() {
        let dir = tmp_dir("path_is_file");
        let file_path = dir.join("some_file");
        std::fs::write(&file_path, b"").unwrap();
        let result = detect_project(&file_path);
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
