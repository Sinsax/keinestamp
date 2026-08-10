use std::path::Path;
use anyhow::Result;
use crate::lore_backend::{self, LoreStatusLine};

pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>> {
    Ok(lore_backend::status_lines(project_root)?
        .into_iter()
        .filter(|f| !crate::hidden::is_hidden(&f.path, hidden))
        .collect())
}

pub fn record(project_root: &Path, message: &str, selected: Option<&[String]>, hidden: &[String]) -> Result<String> {
    let rev = match selected {
        None => lore_backend::commit(project_root, message, &[("kind", "user")])?,
        Some(paths) => {
            let clean: Vec<String> = paths
                .iter()
                .filter(|p| !crate::hidden::is_hidden(p, hidden))
                .cloned()
                .collect();
            lore_backend::commit_files(project_root, message, &[("kind", "user")], &clean)?
        }
    };
    crate::timeline::rebuild_cache(project_root)?;
    Ok(rev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore_backend::{self, create_repo, revision_history};
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_lp_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn require_server() {
        if !lore_backend::server_available() {
            panic!("loreserver 未运行：请先启动 loreserver.exe（demo 模式）");
        }
    }

    #[test]
    fn test_changed_files_filters_hidden() {
        require_server();
        let root = tmp_root("cf");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/b.txt"), "2").unwrap();
        let files = changed_files(&root, &vec!["web/".to_string()]).unwrap();
        assert!(files.iter().all(|f| !f.path.starts_with("web/")));
        assert!(files.iter().any(|f| f.path == "a.txt"));
    }

    #[test]
    fn test_record_commits_and_kind_metadata() {
        require_server();
        let root = tmp_root("rec");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        record(&root, "msg", None, &vec!["web/".to_string()]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("user"));
        assert!(root.join("web/timeline.json").exists());
    }
}
