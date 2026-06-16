use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn is_default_path(p: &PathBuf) -> bool {
    p.as_os_str().is_empty()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaConfig {
    pub version: String,
    pub init_time: String,
    #[serde(default, skip_serializing_if = "is_default_path")]
    pub project_root: PathBuf,
    #[serde(default, skip_serializing_if = "is_default_path")]
    pub web_root: PathBuf,
    #[serde(default)]
    pub web_remote: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub project_id: Option<String>,
}

impl MetaConfig {
    pub fn new(project_root: PathBuf) -> Self {
        let web_root = project_root.join("web-timeline");
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            init_time: chrono::Utc::now().to_rfc3339(),
            project_root,
            web_root,
            web_remote: None,
            access_token: Some(generate_access_token()),
            auto_connect: false,
            project_id: Some(generate_project_id()),
        }
    }
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            version: String::new(),
            init_time: String::new(),
            project_root: PathBuf::new(),
            web_root: PathBuf::new(),
            web_remote: None,
            access_token: None,
            auto_connect: false,
            project_id: None,
        }
    }
}

pub fn generate_project_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    format!("{:016x}{:08x}", n, pid)
}

pub fn generate_access_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let pid = std::process::id();
    let mix = secs.wrapping_mul(u64::from(nanos)) ^ u64::from(pid);
    format!("{:016x}{:08x}{:08x}{:016x}", secs, nanos, pid, mix)
}

pub fn meta_path(project_root: &Path) -> PathBuf {
    project_root.join(".saki-step.toml")
}

pub fn write_meta(path: &Path, config: &MetaConfig) -> Result<()> {
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    std::fs::create_dir_all(parent)?;

    let mut stripped = config.clone();
    stripped.project_root = PathBuf::new();
    stripped.web_root = PathBuf::new();
    let content = toml::to_string_pretty(&stripped)?;

    let tmp_path = parent.join(format!(
        ".saki-step.tmp.{}.toml",
        std::process::id()
    ));
    std::fs::write(&tmp_path, &content)
        .with_context(|| format!("failed to write temporary config to {:?}", tmp_path))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to rename {:?} to {:?}", tmp_path, path))?;

    Ok(())
}

pub fn read_meta(path: &Path) -> Result<MetaConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {:?}", path))?;
    let mut config: MetaConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config file {:?}", path))?;
    if config.project_root.as_os_str().is_empty() {
        let parent = path.parent().context("config path has no parent")?;
        config.project_root = parent.to_path_buf();
    }
    if config.web_root.as_os_str().is_empty() {
        config.web_root = config.project_root.join("web-timeline");
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_path() {
        let root = Path::new("/some/project");
        assert_eq!(
            meta_path(root),
            Path::new("/some/project/.saki-step.toml")
        )
    }

    #[test]
    fn test_write_and_read_meta() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("saki_test_config_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;

        let path = meta_path(&dir);
        let config = MetaConfig::new(dir.clone());

        write_meta(&path, &config)?;
        let read_back = read_meta(&path)?;

        assert_eq!(read_back.version, config.version);
        assert_eq!(read_back.init_time, config.init_time);
        assert_eq!(read_back.project_root, config.project_root);
        assert_eq!(read_back.web_root, config.web_root);
        assert!(read_back.access_token.is_some());
        assert!(!read_back.access_token.as_deref().unwrap().is_empty());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_read_meta_corrupt() {
        let dir = std::env::temp_dir().join(format!("cuke_test_corrupt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let path = meta_path(&dir);
        std::fs::write(&path, "this is not valid toml").unwrap();

        let result = read_meta(&path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_meta_not_found() {
        let path = Path::new("/nonexistent/path/.saki-step.toml");
        let result = read_meta(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_project_id_unique() {
        let id1 = generate_project_id();
        let id2 = generate_project_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 24);
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_project_id_in_new_meta() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("saki_test_pid_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let config = MetaConfig::new(dir.clone());
        assert!(config.project_id.is_some());
        assert_eq!(config.project_id.as_ref().unwrap().len(), 24);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_generate_access_token_length() {
        let t1 = generate_access_token();
        let t2 = generate_access_token();
        assert_eq!(t1.len(), 48);
        assert_eq!(t2.len(), 48);
        assert_ne!(t1, t2);
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_access_token_in_new_meta() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("saki_test_at_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let config = MetaConfig::new(dir.clone());
        assert!(config.access_token.is_some());
        assert_eq!(config.access_token.as_ref().unwrap().len(), 48);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_backward_compat_old_fields() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("saki_test_compat_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;

        let path = meta_path(&dir);
        let old_toml = r#"
version = "0.1.0"
init_time = "2026-01-01T00:00:00Z"
project_remote = "https://example.com/project.git"
page_password = "secret123"
blender_path = "C:\\Blender\\blender.exe"
"#;
        std::fs::write(&path, old_toml)?;

        let config = read_meta(&path)?;
        assert_eq!(config.access_token, None);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}