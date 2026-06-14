use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: String,
    pub last_access: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProjects {
    pub entries: Vec<RecentEntry>,
}

impl RecentProjects {
    pub fn load() -> Self {
        Self::load_from(&default_recent_path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(Self { entries: vec![] }),
            Err(_) => Self { entries: vec![] },
        }
    }

    pub fn save(&self) {
        self.save_to(&default_recent_path());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension(format!(
                "tmp{}_{}",
                std::process::id(),
                path.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            ));
            if std::fs::write(&tmp, &content).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    pub fn add_path(path: &Path) {
        let recent_path = default_recent_path();
        Self::add_path_to(path, &recent_path);
    }

    fn normalize(path: &Path) -> PathBuf {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let s = canonical.to_string_lossy();
        if cfg!(windows) && s.starts_with(r"\\?\") {
            PathBuf::from(&s[4..])
        } else {
            canonical
        }
    }

    pub fn add_path_to(path: &Path, recent_path: &Path) {
        let mut recent = Self::load_from(recent_path);
        let path_str = Self::normalize(path).to_string_lossy().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        recent.entries.retain(|e| e.path != path_str);
        recent.entries.insert(
            0,
            RecentEntry {
                path: path_str,
                last_access: now,
            },
        );
        if recent.entries.len() > MAX_ENTRIES {
            recent.entries.truncate(MAX_ENTRIES);
        }
        recent.save_to(recent_path);
    }

    pub fn remove_path(path: &Path) {
        let recent_path = default_recent_path();
        Self::remove_path_from(path, &recent_path);
    }

    pub fn remove_path_from(path: &Path, recent_path: &Path) {
        let mut recent = Self::load_from(recent_path);
        let path_str = Self::normalize(path).to_string_lossy().to_string();
        recent.entries.retain(|e| e.path != path_str);
        recent.save_to(recent_path);
    }

    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }
}

fn default_recent_path() -> PathBuf {
    let (base, dir_name) = if cfg!(target_os = "windows") {
        (
            std::env::var("APPDATA")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            "SakiStep",
        )
    } else {
        (
            std::env::var("XDG_DATA_HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var("HOME")
                        .ok()
                        .map(|h| PathBuf::from(h).join(".local").join("share"))
                })
                .unwrap_or_else(|| PathBuf::from(".")),
            "saki-step",
        )
    };
    base.join(dir_name).join("recent.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_recent_path() -> PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("saki_recent_test_{}_{}.json", std::process::id(), n))
    }

    #[test]
    fn test_empty_recent() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);
        let recent = RecentProjects::load_from(&path);
        assert!(recent.entries.is_empty());
    }

    #[test]
    fn test_add_and_retrieve() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        RecentProjects::add_path_to(Path::new("/tmp/test_project"), &path);

        let recent = RecentProjects::load_from(&path);
        assert!(!recent.entries.is_empty());
        assert!(recent.entries[0].path.contains("test_project"));
    }

    #[test]
    fn test_dedup_moves_to_front() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        RecentProjects::add_path_to(Path::new("/tmp/proj_a"), &path);
        RecentProjects::add_path_to(Path::new("/tmp/proj_b"), &path);
        RecentProjects::add_path_to(Path::new("/tmp/proj_a"), &path);

        let recent = RecentProjects::load_from(&path);
        assert_eq!(recent.entries.len(), 2);
        assert!(recent.entries[0].path.contains("proj_a"));
    }

    #[test]
    fn test_max_entries() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        for i in 0..25 {
            RecentProjects::add_path_to(
                Path::new(&format!("/tmp/proj_{}", i)),
                &path,
            );
        }

        let recent = RecentProjects::load_from(&path);
        assert_eq!(recent.entries.len(), 20);
        assert!(recent.entries[0].path.contains("proj_24"));
    }

    #[test]
    fn test_paths_method() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        RecentProjects::add_path_to(Path::new("/tmp/proj_x"), &path);
        RecentProjects::add_path_to(Path::new("/tmp/proj_y"), &path);

        let recent = RecentProjects::load_from(&path);
        assert_eq!(recent.paths(), vec!["/tmp/proj_y", "/tmp/proj_x"]);
    }

    #[test]
    fn test_remove_path() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        RecentProjects::add_path_to(Path::new("/tmp/proj_a"), &path);
        RecentProjects::add_path_to(Path::new("/tmp/proj_b"), &path);
        RecentProjects::add_path_to(Path::new("/tmp/proj_c"), &path);

        RecentProjects::remove_path_from(Path::new("/tmp/proj_b"), &path);

        let recent = RecentProjects::load_from(&path);
        assert_eq!(recent.entries.len(), 2);
        assert!(recent.entries[0].path.contains("proj_c"));
        assert!(recent.entries[1].path.contains("proj_a"));
    }

    #[test]
    fn test_remove_path_nonexistent() {
        let path = temp_recent_path();
        let _ = std::fs::remove_file(&path);

        RecentProjects::add_path_to(Path::new("/tmp/proj_a"), &path);

        RecentProjects::remove_path_from(Path::new("/tmp/nonexistent"), &path);

        let recent = RecentProjects::load_from(&path);
        assert_eq!(recent.entries.len(), 1);
    }

    #[test]
    fn test_remove_path_empty_list() {
        let path = temp_recent_path();

        RecentProjects::remove_path_from(Path::new("/tmp/proj"), &path);

        let recent = RecentProjects::load_from(&path);
        assert!(recent.entries.is_empty());
    }
}
