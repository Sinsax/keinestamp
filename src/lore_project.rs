use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::lore_backend::{self, LoreState, LoreStatusLine};

pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>> {
    Ok(lore_backend::status_lines(project_root)?
        .into_iter()
        .filter(|f| !crate::hidden::is_hidden(&f.path, hidden))
        .collect())
}

pub fn record(
    project_root: &Path,
    message: &str,
    tag: Option<&str>,
    selected: Option<&[String]>,
    hidden: &[String],
) -> Result<String> {
    // 回退状态下：X 删除的文件（回退删除标记）在记录时算删除——物理删除 → D 提交。
    // 无论是否勾选（checkbox 只读）都纳入本次提交，否则回退删除永远无法落库。
    let marker = crate::timeline::read_marker(project_root)?;
    let delete_files: Vec<String> = match &marker {
        Some(m) => m
            .deleted_files
            .iter()
            .filter(|p| !crate::hidden::is_hidden(p, hidden))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    let mut paths: Vec<String> = match selected {
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
    for p in &delete_files {
        if !paths.contains(p) {
            paths.push(p.clone());
        }
    }
    if paths.is_empty() {
        bail!("无改动可记录");
    }
    // 校验通过后物理删除（回退删除文件）
    for p in &delete_files {
        let _ = std::fs::remove_file(project_root.join(p));
    }
    // .loreignore 自动记录：UI 隐藏它（load_hidden 预设），但每次记录强制携带其变更进历史。
    // 点开头文件 lore 默认拒绝 stage（Invalid path，实测 stage.rs:172）→ 用 --force 绕过
    // （实测可用；无变化时输出 "No changes staged" 且 exit 0，不产生空提交）
    let _ = lore_backend::run_lore(project_root, &["--force", "stage", crate::hidden::LOREIGNORE]);
    let mut meta: Vec<(&str, &str)> = vec![("kind", "user")];
    if let Some(t) = tag {
        meta.push(("tag", t));
    }
    let rev = lore_backend::commit_files(project_root, message, &meta, &paths)?;
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

/// 回退（v1 语义定稿）：选择 X 回退 = 回退到 X 的父记录（X-1），
/// X 相对 X-1 的改动作为真实未提交修改应用回工作区：
/// - M/A 文件：写回 X 版本内容（状态"回退改动"，可勾选可记录）
/// - D 文件：磁盘保留（状态"回退删除"，只读；记录时算删除）
/// 回退前未提交改动暂存（rollback-backup/，恢复时取回）。
/// 首条记录（无父）特例：reset 到自身——工作区还原为该记录内容
/// （该记录相对"空"的改动检出后与 HEAD 一致，自然无差异）。
/// 连续回退（已有回退态）：**只备份首次回退**；再次回退时回退期间的更改内容
/// （上次回退检出的中间态）需丢弃确认——discard=true 丢弃后继续，false 则报错
/// （GUI 弹窗「丢弃并跳转 / 留下」）；head_before 始终为首回退前。
pub fn rollback(project_root: &Path, revision: &str, hidden: &[String], discard: bool) -> Result<()> {
    let existing = crate::timeline::read_marker(project_root)?;
    // 连续回退：head_before 沿用首次（恢复始终回到最初回退前）；只备份首次回退
    let head_before = match &existing {
        Some(m) => m.head_before.clone(),
        None => lore_backend::current_revision(project_root)?,
    };
    match &existing {
        Some(_) => {
            // 连续回退：只有"真修改"（内容与检出不符）才需丢弃确认；纯检出自动丢弃不弹窗
            let modified = rollback_modified_files(project_root)?;
            if !discard && !modified.is_empty() {
                bail!("回退需要丢弃确认：回退期间有未提交更改（修改过上次回退检出的内容），继续回退将丢弃这些更改");
            }
            discard_dirty(project_root, hidden)?;
        }
        None => {
            let backup_dir = rollback_backup_dir(project_root);
            let _backup = backup_dirty_to(project_root, hidden, &backup_dir)?;
        }
    }
    // 目标 = X 的父（X-1）；首条无父 → reset 到自身（工作区还原为该记录内容）
    let parent = lore_backend::revision_parent(project_root, revision)?;
    let rollback_point = match &parent {
        Some(p) => {
            lore_backend::branch_reset(project_root, p)?;
            p.clone()
        }
        None => {
            lore_backend::branch_reset(project_root, revision)?;
            revision.to_string()
        }
    };
    // 应用 X 改动：M/A 文件用 `file reset --revision X` 检出到工作区（原生，实测可用）；
    // D 文件跳过（磁盘保留，标记回退删除）。首条时 X 相对"空"的改动检出后与 HEAD 一致，无差异。
    let changes = lore_backend::revision_changes(project_root, revision)?;
    let mut reset_paths: Vec<String> = Vec::new();
    let mut deleted_files: Vec<String> = Vec::new();
    let mut change_files: Vec<String> = Vec::new();
    for (status, path) in &changes {
        match status {
            'A' | 'M' => {
                reset_paths.push(path.clone());
                change_files.push(path.clone());
            }
            'D' => deleted_files.push(path.clone()),
            _ => {}
        }
    }
    if !reset_paths.is_empty() {
        let mut args: Vec<&str> = vec!["file", "reset", "--revision", revision];
        for p in &reset_paths {
            args.push(p);
        }
        lore_backend::run_lore(project_root, &args)?;
    }
    // 检出后（工作区 = X 内容）记录每个 change_files 的 sha256，用于判定"真修改"（已检出 vs 已修改）
    let mut change_hashes: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for p in &change_files {
        if let Ok(h) = file_sha256(&project_root.join(p)) {
            change_hashes.insert(p.clone(), h);
        }
    }
    crate::timeline::write_marker(
        project_root,
        &crate::timeline::RollbackMarker {
            rollback_point,
            superseded_point: revision.to_string(),
            head_before,
            deleted_files,
            change_files,
            change_hashes,
        },
    )?;
    Ok(())
}

/// 文件内容 hash（DefaultHasher/SipHash，十六进制）。读不到（文件缺失/IO 错误）返回空串——
/// 与任何检出 hash 都不等 → 视为修改。
fn file_sha256(path: &Path) -> Result<String> {
    let content = match std::fs::read(path) {
        Ok(c) => c,
        Err(_) => return Ok(String::new()),
    };
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

/// 回退状态下的"真修改"文件（连续回退需确认的）：
/// - change_files 中工作区 hash ≠ 检出 hash 的（修改过检出内容）
/// - 不在 change_files 的普通未提交改动（用户编辑非检出文件）
/// 纯检出（内容与检出一致）不算修改。无回退态 → 空。
/// 旧 marker 无 change_hashes 时 fallback 全量 changed_files（保守：视为修改）。
pub fn rollback_modified_files(project_root: &Path) -> Result<Vec<String>> {
    let Some(marker) = crate::timeline::read_marker(project_root)? else {
        return Ok(Vec::new());
    };
    let hidden = crate::hidden::load_hidden(project_root);
    let all = changed_files(project_root, &hidden)?;
    if marker.change_hashes.is_empty() {
        return Ok(all.into_iter().map(|f| f.path).collect());
    }
    let mut out: Vec<String> = Vec::new();
    for p in &marker.change_files {
        let cur = file_sha256(&project_root.join(p))?;
        if marker.change_hashes.get(p).map(String::as_str) != Some(cur.as_str()) {
            out.push(p.clone());
        }
    }
    for f in all {
        if !marker.change_files.contains(&f.path) {
            out.push(f.path);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// 丢弃回退期间的未提交更改（连续回退确认后）：Modified/Deleted 还原到 HEAD，untracked 物理删除。
fn discard_dirty(project_root: &Path, hidden: &[String]) -> Result<()> {
    let files = changed_files(project_root, hidden)?;
    let mut reset_paths: Vec<String> = Vec::new();
    for f in &files {
        match f.state {
            LoreState::Modified | LoreState::Deleted => reset_paths.push(f.path.clone()),
            LoreState::Added => {
                let _ = std::fs::remove_file(project_root.join(&f.path));
            }
            _ => {}
        }
    }
    if !reset_paths.is_empty() {
        let mut args: Vec<&str> = vec!["file", "reset"];
        for p in &reset_paths {
            args.push(p);
        }
        lore_backend::run_lore(project_root, &args)?;
    }
    Ok(())
}

/// 回退冲突检测：回退期间当前工作区改动 ∩ 回退前暂存内容（同名文件两边都有改动）
pub fn rollback_conflicts(project_root: &Path) -> Result<Vec<String>> {
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    let hidden = crate::hidden::load_hidden(project_root);
    let current_files: std::collections::HashSet<String> = changed_files(project_root, &hidden)?
        .into_iter()
        .map(|f| f.path)
        .collect();
    let stashed: std::collections::HashSet<String> = stashed_files(project_root)?
        .into_iter()
        .collect();
    let mut conflicts: Vec<String> = current_files.intersection(&stashed).cloned().collect();
    conflicts.sort();
    let _ = marker;
    Ok(conflicts)
}

/// 撤销回退（恢复）：无冲突时自动带回（回退前暂存取回；检出文件丢弃，用户新编辑带回）。
/// 有冲突时返回错误（前端走 rollback_conflicts + resolve_rollback_conflicts）。
pub fn undo_rollback(project_root: &Path, hidden: &[String]) -> Result<()> {
    let conflicts = rollback_conflicts(project_root)?;
    if !conflicts.is_empty() {
        bail!("回退冲突：{}", conflicts.join(", "));
    }
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    // 无冲突：检出文件（change_files）全部丢弃（恢复 = 回到回退前状态）
    finish_rollback_restore(project_root, hidden, &marker.change_files)?;
    Ok(())
}

/// 冲突解决后恢复：choices = [(path, "current"|"stashed")]——"current"=当前版本改动，保留回退期间内容；
/// "stashed"=回退前改动，用回退前暂存内容。
pub fn resolve_rollback_conflicts(
    project_root: &Path,
    hidden: &[String],
    choices: &[(String, String)],
) -> Result<()> {
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    // 选择"current"的冲突文件：检出内容带回（不排除）；其余检出文件默认丢弃
    let keep_current: std::collections::HashSet<&str> = choices
        .iter()
        .filter(|(_, c)| c == "current")
        .map(|(p, _)| p.as_str())
        .collect();
    let mut exclude_new: Vec<String> = marker
        .change_files
        .iter()
        .filter(|p| !keep_current.contains(p.as_str()))
        .cloned()
        .collect();
    exclude_new.extend(
        choices
            .iter()
            .filter(|(_, c)| c == "stashed")
            .map(|(p, _)| p.clone()),
    );
    finish_rollback_restore(project_root, hidden, &exclude_new)?;
    Ok(())
}

/// 恢复执行：当前改动暂存 → reset head_before → 恢复回退前暂存 + 回退期间改动（exclude_new 跳过）。
/// 检出文件（change_files）默认丢弃（不带回）——回退语义：恢复 = 回到回退前状态。
fn finish_rollback_restore(
    project_root: &Path,
    hidden: &[String],
    exclude_new: &[String],
) -> Result<()> {
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    let current = lore_backend::current_revision(project_root)?;
    if current != marker.rollback_point {
        bail!("回退后已有新记录，无法撤销回退");
    }
    let new_dir = rollback_backup_new_dir(project_root);
    let new_backup = backup_dirty_to(project_root, hidden, &new_dir)?;
    lore_backend::branch_reset(project_root, &marker.head_before)?;
    // 先恢复回退前暂存（全部），再恢复回退期间改动（后写优先；exclude_new = 丢弃项）
    let backup_dir = rollback_backup_dir(project_root);
    let _ = restore_backup_at(project_root, &backup_dir, &[]);
    if new_backup {
        let _ = restore_backup_at(project_root, &new_dir, exclude_new);
    }
    let _ = std::fs::remove_dir_all(&backup_dir);
    let _ = std::fs::remove_dir_all(&new_dir);
    crate::timeline::clear_rollback(project_root)?;
    Ok(())
}

/// 回退暂存目录（仓库内 .keinestamp/，应用层隐藏、跨会话持久）
fn rollback_backup_dir(project_root: &Path) -> PathBuf {
    project_root.join(crate::hidden::PRIVATE_DIR).join("rollback-backup")
}

fn rollback_backup_new_dir(project_root: &Path) -> PathBuf {
    project_root.join(crate::hidden::PRIVATE_DIR).join("rollback-backup-new")
}

/// 清回退标记 + 暂存备份（顶替记录后放弃暂存内容）
pub fn clear_rollback(project_root: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(rollback_backup_dir(project_root));
    let _ = std::fs::remove_dir_all(rollback_backup_new_dir(project_root));
    crate::timeline::clear_rollback(project_root)
}

/// 回退状态下记录 = 顶替（branch reset 后正常 commit，父=回退点），成功后清除回退标记。
pub fn record_after_rollback(
    project_root: &Path,
    message: &str,
    tag: Option<&str>,
    hidden: &[String],
) -> Result<String> {
    let rev = record(project_root, message, tag, None, hidden)?;
    crate::lore_project::clear_rollback(project_root)?;
    Ok(rev)
}

/// kind=web 的 revision 数（部署版本号 hint，替代 v1 web 仓库 rev-list --count）。
/// 非 lore 仓库返回 0（兼容迁移期间项目部署不报错）。
pub fn web_revision_count(project_root: &Path) -> Result<u64> {
    if !lore_backend::repo_exists(project_root) {
        return Ok(0);
    }
    Ok(lore_backend::revision_history(project_root)?
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) == Some("web"))
        .count() as u64)
}

/// 导出 entry 全部文件到指定目录（dump 解析 + 逐文件导出）
pub fn export_entry(project_root: &Path, revision: &str, dest: &Path) -> Result<()> {
    lore_backend::export_files(project_root, revision, dest)
}

/// 该 entry 中某文件相对其父记录的改动 diff 文本
pub fn get_file_diff(project_root: &Path, revision: &str, path: &str) -> Result<String> {
    lore_backend::file_diff(project_root, revision, path)
}

/// 备份未提交改动到指定目录（暂存语义，v1 stash）：
/// - Modified tracked 文件：复制到备份目录（保留相对路径）→ `file reset` 还原到 HEAD
/// - Deleted tracked 文件：记清单（写入 `backup-meta.json` 持久化，供跨进程恢复）→ `file reset` 恢复
/// - untracked 文件：不处理（branch reset 不触碰）
/// 返回是否产生了备份（无改动 → false）。
fn backup_dirty_to(project_root: &Path, hidden: &[String], dir: &Path) -> Result<bool> {
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
        return Ok(false);
    }
    let _ = std::fs::remove_dir_all(dir);
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
    if !deleted.is_empty() {
        // 仅删除无修改时目录尚未创建
        std::fs::create_dir_all(dir)?;
        let meta = serde_json::json!({ "deleted": deleted });
        std::fs::write(dir.join("backup-meta.json"), serde_json::to_string_pretty(&meta)?)?;
    }
    let mut args: Vec<&str> = vec!["file", "reset"];
    for p in &reset_paths {
        args.push(p);
    }
    lore_backend::run_lore(project_root, &args)?;
    Ok(true)
}

/// 恢复暂存内容到工作区（目录存在且有备份时）；恢复后删除备份目录。
/// exclude：恢复时跳过的相对路径（冲突解决中"选择回退前版本"的文件）。
fn restore_backup_at(project_root: &Path, dir: &Path, exclude: &[String]) -> Result<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    let mut deleted: Vec<String> = Vec::new();
    let meta_path = dir.join("backup-meta.json");
    if let Ok(content) = std::fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(arr) = meta.get("deleted").and_then(|d| d.as_array()) {
                deleted = arr.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            }
        }
    }
    let _ = std::fs::remove_file(&meta_path);
    copy_tree_excl(dir, project_root, exclude)?;
    // 删除清单同样遵守 exclude（冲突解决选"回退前改动"的文件不执行删除）
    for p in &deleted {
        if exclude.iter().any(|e| e == p) {
            continue;
        }
        let _ = std::fs::remove_file(project_root.join(p));
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(true)
}

fn copy_tree_excl(src: &Path, dst: &Path, exclude: &[String]) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let rel = from
            .strip_prefix(src)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if exclude.iter().any(|e| e == &rel) {
            continue;
        }
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to)?;
            copy_tree_excl(&from, &to, exclude)?;
        } else {
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 回退前暂存的文件相对路径列表（不含 backup-meta.json）
fn stashed_files(project_root: &Path) -> Result<Vec<String>> {
    let dir = rollback_backup_dir(project_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk_relative(&dir, "", &mut out)?;
    out.retain(|p| p != "backup-meta.json");
    Ok(out)
}

fn walk_relative(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{}/{}", prefix, name) };
        if entry.file_type()?.is_dir() {
            walk_relative(&entry.path(), &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore_backend::{self, commit_web, create_repo, revision_history};
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_lp_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_changed_files_filters_hidden() {
        let root = tmp_root("cf");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::create_dir_all(root.join(".keinestamp").join("web")).unwrap();
        std::fs::write(root.join(".keinestamp/web/b.txt"), "2").unwrap();        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(files.iter().all(|f| !f.path.starts_with(".keinestamp/")));
        assert!(files.iter().any(|f| f.path == "a.txt"));
    }

    #[test]
    fn test_record_commits_and_kind_metadata() {
        let root = tmp_root("rec");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        record(&root, "msg", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("user"));
        assert!(root.join(".keinestamp/web/timeline.json").exists());
    }

    #[test]
    fn test_record_auto_carries_loreignore() {
        let root = tmp_root("recig");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        std::fs::write(root.join(".loreignore"), ".keinestamp/.keinestamp.toml\n").unwrap();
        record(&root, "msg", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // .loreignore 应随记录自动入库（UI 隐藏但自动记录）
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        let out = lore_backend::run_lore(&root, &["file", "write", "--path", ".loreignore", "--revision", &hist[0].id, "--output", &std::env::temp_dir().join("keinestamp_ig_check.txt").to_string_lossy()]).unwrap();
        let _ = out;
        let saved = std::fs::read_to_string(std::env::temp_dir().join("keinestamp_ig_check.txt")).unwrap();
        assert!(saved.contains(".keinestamp/.keinestamp.toml"), ".loreignore 应被自动提交: {}", saved);
        let _ = std::fs::remove_file(std::env::temp_dir().join("keinestamp_ig_check.txt"));
        // 无 .loreignore 文件时 record 也应正常（不强制创建）
        let root2 = tmp_root("recig2");
        create_repo(&root2).unwrap();
        std::fs::write(root2.join("a.txt"), "y").unwrap();
        record(&root2, "msg2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist2 = revision_history(&root2).unwrap();
        assert_eq!(hist2.len(), 1);
    }

    #[test]
    fn test_record_with_tag_writes_metadata() {
        let root = tmp_root("rectag");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        record(&root, "msg", Some("建模"), None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist[0].metadata.get("tag").map(String::as_str), Some("建模"));
        // timeline 缓存也应带 tag
        let timeline = crate::timeline::read_timeline(&crate::timeline::timeline_cache_path(&root)).unwrap();
        assert_eq!(timeline.entries[0].tag.as_deref(), Some("建模"));
    }

    #[test]
    fn test_web_commit_and_count() {
        let root = tmp_root("webcnt");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "user").unwrap();
        record(&root, "user record", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(web_revision_count(&root).unwrap(), 0);
        std::fs::create_dir_all(root.join(".keinestamp").join("web")).unwrap();
        std::fs::write(root.join(".keinestamp/web/index.html"), "<html></html>").unwrap();
        commit_web(&root).unwrap();
        assert_eq!(web_revision_count(&root).unwrap(), 1);
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist.len(), 2);
        assert!(hist.iter().any(|r| r.metadata.get("kind").map(String::as_str) == Some("user")), "用户记录应保留 kind=user");
        assert!(hist.iter().any(|r| r.metadata.get("kind").map(String::as_str) == Some("web")), "web 提交应为 kind=web");
    }

    #[test]
    fn test_web_revision_count_zero_on_non_lore_repo() {
        let root = tmp_root("webcnt_nolore");
        assert_eq!(web_revision_count(&root).unwrap(), 0);
    }

    #[test]
    fn test_rollback_undo_supersede_flow() {
        let root = tmp_root("rb");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 选 r3 回退 → 回退到 r2（r3 的父），r3 的改动（a.txt v3）作为未提交修改
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "工作区 = r3 内容（未提交）");
        assert!(rollback_state(&root).unwrap().active);

        // 撤销回退（回到 v3 的提交状态 HEAD=r3）
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3, "HEAD 恢复为 r3");

        // 再选 r2 回退 → 回退到 r1，r2 改动未提交 → 修改后记录顶替
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("a.txt"), "v4-supersede").unwrap();
        let r4 = record_after_rollback(&root, "r4", None, &vec![".keinestamp/".to_string()]).unwrap();
        assert_ne!(r4, r3);
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist[0].message, "r4"); // 最新 revision 是 r4（r2/r3 被顶替出主线）
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_stashes_dirty_and_undo_restores() {
        let root = tmp_root("rbdirty");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "b1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // dirty: b.txt 改动（r2 未改 b.txt，与检出文件 a.txt 不冲突）+ untracked 新文件
        std::fs::write(root.join("b.txt"), "B-DIRTY").unwrap();
        std::fs::write(root.join("new.txt"), "n").unwrap();
        // 选 r2 回退 → 回退到 r1，r2 改动（a.txt v2）检出为未提交；
        // 回退前 dirty（b.txt B-DIRTY）暂存移出工作区
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2", "工作区 = r2 检出内容");
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "b1", "回退前 dirty 暂存移出（工作区 = r1 的 b.txt）");
        assert!(root.join("new.txt").exists(), "untracked 文件保留在磁盘");
        assert!(rollback_state(&root).unwrap().active);
        // 撤销回退：HEAD 回 r2 + 暂存 dirty 取回（无冲突自动带回）
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "B-DIRTY", "暂存 dirty 取回");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_conflict_resolve() {
        let root = tmp_root("rbconf");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // dirty: a.txt（与 r2 检出文件冲突）
        std::fs::write(root.join("a.txt"), "A-DIRTY").unwrap();
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // 检出 r2 的 a.txt=v2 覆盖了 dirty（dirty 已暂存）
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        // 冲突检测：当前改动（a.txt v2 检出）∩ 暂存（a.txt A-DIRTY）
        let conflicts = rollback_conflicts(&root).unwrap();
        assert_eq!(conflicts, vec!["a.txt".to_string()]);
        // undo 直接调用应报冲突
        assert!(undo_rollback(&root, &vec![".keinestamp/".to_string()]).is_err());
        // resolve：选择"回退前改动"（stashed）→ 恢复后 a.txt = A-DIRTY
        resolve_rollback_conflicts(
            &root,
            &vec![".keinestamp/".to_string()],
            &[("a.txt".to_string(), "stashed".to_string())],
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "A-DIRTY", "回退前改动取回");
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_conflict_deleted_file_stashed_choice_restores() {
        let root = tmp_root("rbconfdel");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退前 dirty a.txt（A-DIRTY，暂存进 backup）
        std::fs::write(root.join("a.txt"), "A-DIRTY").unwrap();
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // 回退期间用户删除 a.txt（工作区 D）
        std::fs::remove_file(root.join("a.txt")).unwrap();
        // 冲突：当前改动（D a.txt）∩ 暂存（a.txt A-DIRTY）
        let conflicts = rollback_conflicts(&root).unwrap();
        assert_eq!(conflicts, vec!["a.txt".to_string()]);
        // resolve 选"回退前改动"（stashed）→ a.txt 应恢复为 A-DIRTY（不被 new_dir 删除清单误删）
        resolve_rollback_conflicts(
            &root,
            &vec![".keinestamp/".to_string()],
            &[("a.txt".to_string(), "stashed".to_string())],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "A-DIRTY",
            "选择回退前改动后文件应恢复（删除清单不覆盖 exclude）"
        );
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_to_latest_applies_changes() {
        let root = tmp_root("rbhead");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 选 r2（最新）回退 → 回退到 r1，r2 的改动（v2）作为未提交修改
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1, "HEAD = r1（r2 的父）");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2", "r2 改动检出为未提交");
        // status 应显示 a.txt 已修改
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(files.iter().any(|f| f.path == "a.txt"), "r2 检出改动应为未提交修改");
        // 撤销回退：HEAD 回 r2，检出改动丢弃（工作区 = r2 内容，干净）
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_first_entry_resets_to_self() {
        let root = tmp_root("rbfirst");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首条 r1 回退：无父 → reset 到自身（工作区 = r1 内容）；r1 相对"空"的改动检出后与 HEAD 一致，无差异
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1, "HEAD = r1（首条自身）");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1", "工作区 = r1 内容");
        assert!(rollback_state(&root).unwrap().active);
        // status 无改动（检出内容与 HEAD 一致）
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(files.is_empty(), "首条回退后应无未提交修改");
        // marker：rollback_point == superseded_point == r1
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.rollback_point, r1);
        assert_eq!(marker.superseded_point, r1);
        assert!(marker.change_files.is_empty());
        assert!(marker.deleted_files.is_empty());

        // 撤销回退：HEAD 回 r3 + 内容还原
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3, "HEAD 恢复为 r3");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r2;
    }

    #[test]
    fn test_rollback_first_entry_supersede() {
        let root = tmp_root("rbfirstsup");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首条回退后修改 → 记录顶替（新 revision 父 = r1）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("a.txt"), "v4").unwrap();
        let r4 = record_after_rollback(&root, "r4", None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist[0].message, "r4");
        assert_eq!(hist.len(), 2, "主线只剩 r4 + r1（r2/r3 顶替出主线）");
        assert_eq!(lore_backend::revision_parent(&root, &r4).unwrap().as_deref(), Some(r1.as_str()), "r4 父 = r1");
        assert!(!rollback_state(&root).unwrap().active);
        // timeline 中 r2/r3 标记 superseded（折叠）
        let tl = crate::timeline::read_timeline(&crate::timeline::timeline_cache_path(&root)).unwrap();
        let sup: Vec<String> = tl.entries.iter().filter(|e| e.superseded).map(|e| e.message.clone()).collect();
        assert_eq!(sup, vec!["r2".to_string(), "r3".to_string()]);
    }

    #[test]
    fn test_rollback_superseded_entry() {
        let root = tmp_root("rbsupentry");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退 r2 → record r4 顶替（主线 r4 + r1，r2/r3 被顶替出主线）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("a.txt"), "v4").unwrap();
        let r4 = record_after_rollback(&root, "r4", None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退被顶替出主线的 r3（折叠区记录）：reset 到其父 r2 + 检出 r3 改动（a.txt v3）
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD = r3 的父 r2");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "r3 改动检出");
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.rollback_point, r2);
        assert_eq!(marker.superseded_point, r3);
        // 撤销回退：HEAD 回 r4（回退前）
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r4);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v4");
        let _ = r1;
    }

    #[test]
    fn test_rollback_consecutive_requires_discard() {
        let root = tmp_root("rbcons");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首次回退 r2（备份到主目录，head_before = r3）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        let m1 = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(m1.head_before, r3);
        // 纯检出：修改判定为空（检出内容 == hash）
        assert!(rollback_modified_files(&root).unwrap().is_empty(), "纯检出不算修改");
        // 用户真修改检出文件 a.txt
        std::fs::write(root.join("a.txt"), "V2-EDITED").unwrap();
        assert_eq!(rollback_modified_files(&root).unwrap(), vec!["a.txt".to_string()]);
        // 连续回退 r3：真修改未确认时报错
        let err = rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap_err();
        assert!(err.to_string().contains("需要丢弃确认"), "err: {err}");
        // 连续回退 r3 仍处于 r2 回退态（未跳转）
        assert!(rollback_state(&root).unwrap().active);
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1);
        // 丢弃确认后继续回退：head_before 沿用首次（r3）→ 恢复回到最初
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], true).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD = r3 的父 r2");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "r3 改动检出");
        let m2 = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(m2.head_before, r3, "head_before 始终为首回退前");
        // 恢复：回到最初回退前（r3），工作区干净无中间态残留
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_checked_out_not_modified() {
        let root = tmp_root("rbchecked");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退 r2 → 纯检出（a.txt = v2）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(rollback_modified_files(&root).unwrap(), Vec::<String>::new(), "纯检出不算修改");
        // 连续回退 r3：只显示"已检出"不阻碍回退 → 不弹窗自动丢弃检出继续
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD = r3 的父 r2");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "r3 改动检出");
        // 恢复：回到最初（r3）干净
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_consecutive_discard_drops_mid_edits() {
        let root = tmp_root("rbconsedit");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "b1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首次回退 r2 → 回退期间用户编辑 b.txt（非检出文件，也算真修改）→ 连续回退 r3 需确认
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("b.txt"), "B-MID").unwrap();
        assert_eq!(rollback_modified_files(&root).unwrap(), vec!["b.txt".to_string()], "编辑非检出文件也算修改");
        let err = rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap_err();
        assert!(err.to_string().contains("需要丢弃确认"), "err: {err}");
        // 丢弃确认后继续
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], true).unwrap();
        // 丢弃：b.txt 中间编辑被还原到 HEAD（r2 树 = b1）
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "b1", "中间编辑已丢弃");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "r3 改动检出");
        // 恢复：回到最初（r3），无 b.txt 残留
        undo_rollback(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3);
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "b1");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_export_entry_and_diff() {
        let root = tmp_root("pe");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "y").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        let dest = tmp_root("pe_dest");
        export_entry(&root, &r1, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "x");

        let diff = get_file_diff(&root, &r2, "a.txt").unwrap();
        assert!(diff.contains("+++"));
    }
}
