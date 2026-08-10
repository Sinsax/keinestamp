use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::lore_backend::{self, LoreState, LoreStatusLine};

pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>> {
    Ok(lore_backend::status_lines(project_root)?
        .into_iter()
        .filter(|f| !crate::hidden::is_hidden(&f.path, hidden))
        .collect())
}

pub fn record(project_root: &Path, message: &str, selected: Option<&[String]>, hidden: &[String]) -> Result<String> {
    let paths: Vec<String> = match selected {
        None => changed_files(project_root, hidden)?
            .into_iter()
            .map(|f| f.path)
            .collect(),
        Some(sel) => sel
            .iter()
            .filter(|p| !crate::hidden::is_hidden(p, hidden))
            .cloned()
            .collect(),
    };
    if paths.is_empty() {
        bail!("无改动可记录");
    }
    let rev = lore_backend::commit_files(project_root, message, &[("kind", "user")], &paths)?;
    crate::timeline::rebuild_cache(project_root)?;
    Ok(rev)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackState {
    pub active: bool,
    pub revision: Option<String>,
}

pub fn rollback_state(project_root: &Path) -> Result<RollbackState> {
    Ok(match crate::timeline::read_marker(project_root)? {
        Some(m) => RollbackState { active: true, revision: Some(m.rollback_point) },
        None => RollbackState { active: false, revision: None },
    })
}

/// 回退到指定记录（branch reset）。工作区未提交改动先备份（复制到临时目录），
/// 回退后恢复（Lore 拒绝在 dirty 时 reset，Blender 打开文件场景必须保留改动）。
pub fn rollback(project_root: &Path, revision: &str, hidden: &[String]) -> Result<()> {
    let head_before = lore_backend::current_revision(project_root)?;
    if head_before == revision {
        bail!("已在该记录上，无需回退");
    }
    let backup = backup_dirty(project_root, hidden)?;
    let reset = lore_backend::branch_reset(project_root, revision);
    if let Some(b) = &backup {
        let _ = restore_backup(project_root, b);
    }
    reset?;
    crate::timeline::write_marker(
        project_root,
        &crate::timeline::RollbackMarker { rollback_point: revision.to_string(), head_before },
    )?;
    Ok(())
}

/// 撤销回退（branch reset 回回退前 HEAD）。要求 HEAD 仍在回退点（未顶替新记录）。
pub fn undo_rollback(project_root: &Path, hidden: &[String]) -> Result<()> {
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    let current = lore_backend::current_revision(project_root)?;
    if current != marker.rollback_point {
        bail!("回退后已有新记录，无法撤销回退");
    }
    let backup = backup_dirty(project_root, hidden)?;
    let reset = lore_backend::branch_reset(project_root, &marker.head_before);
    if let Some(b) = &backup {
        let _ = restore_backup(project_root, b);
    }
    reset?;
    crate::timeline::clear_rollback(project_root)?;
    Ok(())
}

/// 回退状态下记录 = 顶替（branch reset 后正常 commit，父=回退点），成功后清除回退标记。
pub fn record_after_rollback(project_root: &Path, message: &str, hidden: &[String]) -> Result<String> {
    let rev = record(project_root, message, None, hidden)?;
    crate::timeline::clear_rollback(project_root)?;
    Ok(rev)
}

/// 清回退标记（CLI/Tauri record 成功路径调用）
pub fn clear_rollback(project_root: &Path) -> Result<()> {
    crate::timeline::clear_rollback(project_root)
}

struct DirtyBackup {
    dir: PathBuf,
    deleted: Vec<String>,
}

/// 备份未提交改动并还原工作区，使 branch reset 可通过：
/// - Modified tracked 文件：复制到临时目录（保留相对路径）→ `file reset` 还原到 HEAD
/// - Deleted tracked 文件：记清单 → `file reset` 恢复
/// - untracked 文件：不处理（branch reset 不触碰）
fn backup_dirty(project_root: &Path, hidden: &[String]) -> Result<Option<DirtyBackup>> {
    let files = changed_files(project_root, hidden)?;
    let mut reset_paths: Vec<String> = Vec::new();
    let mut deleted = Vec::new();
    let mut backup_files = Vec::new();
    for f in &files {
        match f.state {
            LoreState::Modified => {
                backup_files.push(f.path.clone());
                reset_paths.push(f.path.clone());
            }
            LoreState::Deleted => {
                deleted.push(f.path.clone());
                reset_paths.push(f.path.clone());
            }
            LoreState::Added => bail!("存在已暂存的改动（staged state），请先处理后回退"),
            _ => {}
        }
    }
    if reset_paths.is_empty() {
        return Ok(None);
    }
    let dir = std::env::temp_dir().join(format!("keinestamp_rollback_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for p in &backup_files {
        let src = project_root.join(p);
        if src.is_file() {
            let dst = dir.join(p);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
        }
    }
    let mut args: Vec<&str> = vec!["file", "reset"];
    for p in &reset_paths {
        args.push(p);
    }
    lore_backend::run_lore(project_root, &args)?;
    Ok(Some(DirtyBackup { dir, deleted }))
}

fn restore_backup(project_root: &Path, backup: &DirtyBackup) -> Result<()> {
    if backup.dir.exists() {
        copy_tree(&backup.dir, project_root)?;
    }
    for p in &backup.deleted {
        let _ = std::fs::remove_file(project_root.join(p));
    }
    let _ = std::fs::remove_dir_all(&backup.dir);
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to)?;
            copy_tree(&from, &to)?;
        } else {
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
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

    #[test]
    fn test_rollback_undo_supersede_flow() {
        require_server();
        let root = tmp_root("rb");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, &vec!["web/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, &vec!["web/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, &vec!["web/".to_string()]).unwrap();

        // 回退到 r2（工作区应回到 v2 状态）
        rollback(&root, &r2, &vec!["web/".to_string()]).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        assert!(rollback_state(&root).unwrap().active);

        // 撤销回退（回到 v3）
        undo_rollback(&root, &vec!["web/".to_string()]).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");

        // 再回退到 r2 然后顶替记录（新记录取代 r3）
        rollback(&root, &r2, &vec!["web/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v4-supersede").unwrap();
        let r4 = record_after_rollback(&root, "r4", &vec!["web/".to_string()]).unwrap();
        assert_ne!(r4, r3);
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist[0].message, "r4"); // 最新 revision 是 r4（r3 被顶替出主线）
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_keeps_dirty_changes() {
        require_server();
        let root = tmp_root("rbdirty");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, &vec!["web/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, &vec!["web/".to_string()]).unwrap();

        // dirty: tracked 文件改动 + untracked 新文件
        std::fs::write(root.join("a.txt"), "DIRTY").unwrap();
        std::fs::write(root.join("new.txt"), "n").unwrap();
        rollback(&root, &r1, &vec!["web/".to_string()]).unwrap();
        // 未提交改动应保留（文件 = DIRTY，而非回退点 v1）
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "DIRTY");
        assert!(root.join("new.txt").exists());
        assert!(rollback_state(&root).unwrap().active);
        // 撤销回退也保留 dirty
        undo_rollback(&root, &vec!["web/".to_string()]).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "DIRTY");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r2;
    }
}
