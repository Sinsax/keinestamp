use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::lore_backend::{self, LoreState, LoreStatusLine};

pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>> {
    Ok(lore_backend::status_lines(project_root)?
        .into_iter()
        .filter(|f| !crate::hidden::is_hidden(&f.path, hidden))
        // 目录行（`A out/`）需过滤：lore 把目录作为一等条目，空目录/未跟踪目录都会出
        // 目录行 → 变更面板出现"幻影文件"。文件行足以在树中呈现目录节点；与 record/
        // commit_files 的过滤语义一致（四层防御第 2 层）
        .filter(|f| !f.path.ends_with('/'))
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
    // 目录路径丢弃（`lore stage <目录>` = stage 目录下全部 dirty 文件——勾选 UI 的目录
    // 残留会让未勾选文件被提交；stage 必须逐文件路径）。目录由前端树展示，不参与提交。
    paths.retain(|p| !p.ends_with('/'));
    if paths.is_empty() {
        bail!("无改动可记录");
    }
    // 校验通过后物理删除（回退删除文件）
    for p in &delete_files {
        let _ = std::fs::remove_file(project_root.join(p));
    }
    // .loreignore 自动记录由 commit_files 统一处理（UI 隐藏它，但每次提交强制携带其变更进历史）
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
/// 从指定 revision 写回单个文件到工作区（检出写回 / 失败还原用）。
/// lore file write 不创建父目录（export_files 同）→ 先建父目录；且 `--output` 不允许覆盖
/// 已存在文件（实测 invalid path）→ 先删再写。
fn write_back(project_root: &Path, path: &str, revision: &str) -> Result<()> {
    let target = project_root.join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&target);
    lore_backend::run_lore(
        project_root,
        &[
            "file",
            "write",
            "--path",
            path,
            "--revision",
            revision,
            "--output",
            target.to_string_lossy().as_ref(),
        ],
    )?;
    Ok(())
}

/// 指定 revision 的树文件列表（`repository dump --revision`），过滤 `.keinestamp/`、`.loreignore`
/// 与目录行。dump 行格式 `<path> id <N> ... addr ...`——**路径可能含空格**（`x 拷贝.png`）→
/// 用 `" id "` 分隔取完整路径（split_whitespace 会截断）；` addr ` 区分文件行（目录行无 addr）。
fn dump_tree_files(project_root: &Path, revision: &str) -> Result<Vec<String>> {
    let out = lore_backend::run_lore(
        project_root,
        &["repository", "dump", "--revision", revision],
    )?;
    let mut files: Vec<String> = Vec::new();
    for line in out.lines() {
        if !line.contains(" addr ") {
            continue;
        }
        let Some(sep) = line.find(" id ") else { continue };
        let path = &line[..sep];
        if path.ends_with('/')
            || path.starts_with(".keinestamp/")
            || path == crate::hidden::LOREIGNORE
        {
            continue;
        }
        files.push(path.to_string());
    }
    // 前缀匹配：某路径是其他路径的前缀 → 目录行（无 / 结尾的情况），丢弃
    Ok(files
        .iter()
        .filter(|p| {
            !files
                .iter()
                .any(|q| q != *p && q.starts_with(&format!("{}/", p)))
        })
        .cloned()
        .collect())
}

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
    let first_backup = existing.is_none();
    let result = rollback_inner(project_root, revision, hidden, discard, &existing, &head_before);
    if result.is_err() && first_backup {
        // 首次回退失败：把已备份的 dirty 还原回工作区（否则文件"消失"，
        // 下次回退 backup_dirty_to 清空备份目录会导致数据丢失）；
        // HEAD 可能已被 branch_reset 移动（后续检出步骤失败）→ 尽力还原回 head_before
        let _ = restore_backup_at(project_root, &rollback_backup_dir(project_root), &[]);
        if let Ok(cur) = lore_backend::current_revision(project_root) {
            if cur != head_before {
                let _ = lore_backend::branch_reset(project_root, &head_before);
            }
        }
    }
    result
}

fn rollback_inner(
    project_root: &Path,
    revision: &str,
    hidden: &[String],
    discard: bool,
    existing: &Option<crate::timeline::RollbackMarker>,
    head_before: &str,
) -> Result<()> {
    let t_start = std::time::Instant::now();
    match existing {
        Some(_) => {
            // 连续回退：只有"真修改"（内容与检出不符）才需丢弃确认；纯检出自动丢弃不弹窗
            let t1 = std::time::Instant::now();
            let modified = rollback_modified_files(project_root)?;
            eprintln!("[rb-t] modified_files: {}ms", t1.elapsed().as_millis());
            if !discard && !modified.is_empty() {
                bail!("回退需要丢弃确认：回退期间有未提交更改（修改过上次回退检出的内容），继续回退将丢弃这些更改");
            }
            discard_dirty(project_root, hidden)?;
        }
        None => {
            let t1 = std::time::Instant::now();
            let backup_dir = rollback_backup_dir(project_root);
            let _backup = backup_dirty_to(project_root, hidden, &backup_dir)?;
            eprintln!("[rb-t] backup_dirty: {}ms", t1.elapsed().as_millis());
        }
    }
    // 目标 = X 的父（X-1）；首条无父 → reset 到自身（工作区还原为该记录内容）
    let t2 = std::time::Instant::now();
    let parent = lore_backend::revision_parent(project_root, revision)?;
    eprintln!("[rb-t] revision_parent: {}ms", t2.elapsed().as_millis());
    let mut backfilled_init: Option<String> = None;
    let mut backfilled_files: Vec<String> = Vec::new();
    let rollback_point = match &parent {
        Some(p) => {
            let t3 = std::time::Instant::now();
            reset_loreignore_to(project_root, p)?;
            lore_backend::branch_reset(project_root, p)?;
            eprintln!("[rb-t] branch_reset: {}ms", t3.elapsed().as_millis());
            p.clone()
        }
        None => {
            // 旧项目首条记录无父（无 init 提交）：reset 自身后，把 HEAD 树（= 该记录）的用户文件
            // 从树中移除——物理删除（staged D）→ 提交空提交（kind=init，仅 .loreignore）→
            // 再从该记录写回 → 文件全部变 untracked（changes 区可重新勾选/修改）。
            // 任一步失败 → 还原（unstage + HEAD 还原，工作区完整恢复）——回退失败不丢文件。
            let t3 = std::time::Instant::now();
            reset_loreignore_to(project_root, revision)?;
            lore_backend::branch_reset(project_root, revision)?;
            let backfill = || -> Result<(Vec<String>, String)> {
                // 必须指定 --revision：不带时 dump 会受 `status --scan` 持久化的 untracked 标志
                // 污染（含已移出的文件），导致误删用户文件/写回失败
                let remove_paths = dump_tree_files(project_root, revision)?;
                // 物理删除：NotFound（磁盘已无此文件——用户改名/移动）视为已删（stage D 正常）；
                // 其他错误（占用/权限）→ 明确报错并还原（文件已删的写回 + unstage）
                let mut removed_ok: Vec<String> = Vec::new();
                for p in &remove_paths {
                    match std::fs::remove_file(project_root.join(p)) {
                        Ok(_) => removed_ok.push(p.clone()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            for done in &removed_ok {
                                let _ = write_back(project_root, done, revision);
                            }
                            let _ = lore_backend::unstage_all(project_root);
                            bail!(
                                "无法删除文件（可能被占用）：{} — {}；请关闭占用该文件的程序后重试",
                                p,
                                e
                            );
                        }
                    }
                }
                // stage 全部（含磁盘缺失的——D 状态；全部 remove 成功/缺失才走到这，无"未变更"路径）
                if !remove_paths.is_empty() {
                    let mut args: Vec<&str> = vec!["stage"];
                    for p in &remove_paths {
                        args.push(p);
                    }
                    if let Err(e) = lore_backend::run_lore(project_root, &args) {
                        for done in &removed_ok {
                            let _ = write_back(project_root, done, revision);
                        }
                        let _ = lore_backend::unstage_all(project_root);
                        return Err(e);
                    }
                }
                let init_id = lore_backend::force_init_commit(project_root).map_err(|e| {
                    for done in &removed_ok {
                        let _ = write_back(project_root, done, revision);
                    }
                    let _ = lore_backend::unstage_all(project_root);
                    e
                })?;
                // 写回该记录的文件（untracked，内容 = 检出版，等价"检出"）
                for p in &remove_paths {
                    write_back(project_root, p, revision)?;
                }
                Ok((remove_paths, init_id))
            };
            let (remove_paths, init_id) = match backfill() {
                Ok(v) => v,
                Err(e) => {
                    // 还原：unstage + HEAD 还原（branch reset 回 head_before → 工作区完整恢复）
                    let _ = lore_backend::unstage_all(project_root);
                    if let Ok(cur) = lore_backend::current_revision(project_root) {
                        if cur != head_before {
                            let _ = lore_backend::branch_reset(project_root, head_before);
                        }
                    }
                    return Err(e);
                }
            };
            backfilled_files = remove_paths;
            backfilled_init = Some(init_id.clone());
            eprintln!("[rb-t] branch_reset(first)+init: {}ms", t3.elapsed().as_millis());
            init_id
        }
    };
    // 检出文件 = 回退点（rollback_point）树的全部文件——统一语义（不做忽略特例）：
    // 忽略文件/两版共有的文件都算检出（工作区残留的忽略文件被写回覆盖为回退点版本）；
    // D 文件（回退点树无）由 deleted_files 标记（磁盘保留，记录时算删除）。
    let t4 = std::time::Instant::now();
    let changes = lore_backend::revision_changes(project_root, revision)?;
    eprintln!("[rb-t] revision_changes: {}ms", t4.elapsed().as_millis());
    let mut deleted_files: Vec<String> = Vec::new();
    for (status, path) in &changes {
        if *status == 'D' {
            deleted_files.push(path.clone());
        }
    }
    // 无父补 init 场景（None 分支）：change_files = 写回的文件列表（工作区 = 检出版）
    let change_files: Vec<String> = if backfilled_init.is_some() {
        backfilled_files.clone()
    } else {
        dump_tree_files(project_root, revision)?
    };
    // 统一检出：写回全部 change_files（从所选版本 X）——file write 建父目录；忽略文件也被
    // 覆盖为与所选版本一致（branch reset 不处理忽略文件，只能手动写回）
    if !change_files.is_empty() {
        let t5 = std::time::Instant::now();
        for p in &change_files {
            write_back(project_root, p, revision)?;
        }
        eprintln!("[rb-t] write_back: {}ms", t5.elapsed().as_millis());
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
            head_before: head_before.to_string(),
            deleted_files,
            change_files,
            change_hashes,
        },
    )?;
    eprintln!("[rb-t] total: {}ms", t_start.elapsed().as_millis());
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
    // 目录行（`A refer/`）不算真修改——目录状态由 change_files 的文件覆盖（目录行会被误判）
    let all = changed_files(project_root, &hidden)?
        .into_iter()
        .filter(|f| !f.path.ends_with('/'))
        .collect::<Vec<_>>();
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
        // 未跟踪不在树 → 不算树的修改（残留/副本/新建不阻碍丢弃/恢复判定；
        // untracked 检出文件已在 change_files 循环处理）
        if f.state == LoreState::Untracked {
            continue;
        }
        if !marker.change_files.contains(&f.path) {
            out.push(f.path);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// 丢弃回退期间的未提交更改（连续回退确认后）：Modified/Deleted 还原到 HEAD，untracked 物理删除。
/// staged（Added）与首次回退的 backup_dirty_to 一致：bail（staged state 会阻止 branch reset，不物理删文件）。
fn discard_dirty(project_root: &Path, hidden: &[String]) -> Result<()> {
    let files = changed_files(project_root, hidden)?;
    let mut reset_paths: Vec<String> = Vec::new();
    for f in &files {
        match f.state {
            LoreState::Modified | LoreState::Deleted => reset_paths.push(f.path.clone()),
            LoreState::Untracked => {
                // 回退期间新建的 untracked（首次回退时工作区 untracked 已被备份移出）——
                // 确认丢弃 = 物理删除（与弹窗文案一致）
                let _ = std::fs::remove_file(project_root.join(&f.path));
            }
            LoreState::Added => bail!("存在已暂存的改动（staged state），请先处理后回退"),
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

/// 取消单个文件的更改（git restore 语义）：`lore file reset <path>` 还原到 HEAD——
/// M（已修改）覆盖回最近记录版本 / D（已删除）从 HEAD 恢复。
/// 仅工作区改动（前端菜单按状态码 M/D 控制；staged/untracked 不在此范围）。
pub fn discard_file_changes(project_root: &Path, path: &str) -> Result<()> {
    if path.is_empty() || path.ends_with('/') {
        bail!("无效路径");
    }
    let full = project_root.join(path);
    if let Some(parent) = full.parent() {
        // file reset 不创建父目录（实测 os error 2）——D 恢复嵌套路径时须先建
        std::fs::create_dir_all(parent)?;
    }
    lore_backend::run_lore(project_root, &["file", "reset", path])?;
    Ok(())
}

/// branch reset 前把 `.loreignore` 还原到目标树版本：它是自动记录文件（hidden 过滤看不到
/// dirty），但 lore 的 reset 校验其内容（工作区 vs 目标树不等会拒绝）。目标树必有该文件
/// （每次记录自动 stage 提交）→ `file reset --revision <目标>` 精确还原（实测无需 --force）。
fn reset_loreignore_to(project_root: &Path, target: &str) -> Result<()> {
    lore_backend::run_lore(
        project_root,
        &["file", "reset", "--revision", target, crate::hidden::LOREIGNORE],
    )?;
    Ok(())
}

/// 回退冲突检测：**真修改**（用户回退后改过的内容，排除纯检出）∩ 回退前暂存内容
/// （同名文件两边都有用户内容）。纯检出 + 回退前 dirty → 无冲突（恢复直接执行：
/// 检出默认丢弃、回退前暂存自动取回）。
pub fn rollback_conflicts(project_root: &Path) -> Result<Vec<String>> {
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    let current_files: std::collections::HashSet<String> = rollback_modified_files(project_root)?
        .into_iter()
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
/// 恢复确认：changes 区有"已修改/删除"内容（非纯检出）时——
/// confirm=None（未确认）→ 报错；Some(true) = 丢弃（修改与回退删除内容不取回）；
/// Some(false) = 留下（已修改的检出取回，删除标记文件保留）。纯检出时 confirm 忽略直接执行。
pub fn undo_rollback(
    project_root: &Path,
    hidden: &[String],
    confirm: Option<bool>,
) -> Result<()> {
    let conflicts = rollback_conflicts(project_root)?;
    if !conflicts.is_empty() {
        eprintln!("[restore] 冲突文件: {:?}", conflicts);
        bail!("回退冲突：{}", conflicts.join(", "));
    }
    let marker = crate::timeline::read_marker(project_root)?.context("无回退状态")?;
    let modified = rollback_modified_files(project_root)?;
    // 确认判定只看"真修改"（用户改过/删过内容）；回退删除标记（deleted_files）属检出
    // 的一部分（恢复时由 branch_reset 自动处理）→ 不触发确认
    if !modified.is_empty() && confirm.is_none() {
        bail!(
            "恢复需要确认：回退期间有已修改内容（{}），继续恢复将丢弃这些更改",
            modified.join(", ")
        );
    }
    eprintln!(
        "[restore] rollback_point={} head_before={} change_files={:?}",
        &marker.rollback_point[..7.min(marker.rollback_point.len())],
        &marker.head_before[..7.min(marker.head_before.len())],
        marker.change_files
    );
    // 检出文件默认丢弃（恢复 = 回到回退前状态）；确认"留下"时已修改的检出取回
    let mut exclude_new = marker.change_files.clone();
    if confirm == Some(true) {
        for p in &marker.deleted_files {
            if !exclude_new.contains(p) {
                exclude_new.push(p.clone());
            }
        }
    } else {
        exclude_new.retain(|p| !modified.contains(p));
    }
    let rollback_point = marker.rollback_point.clone();
    let result = finish_rollback_restore(project_root, hidden, &exclude_new);
    if result.is_err() {
        // 恢复失败：HEAD 可能已被 branch_reset 移动（后续取回步骤失败）→ 尽力还原回回退点
        // （marker 保留，避免"恢复按钮 disabled + 状态不一致"卡死）
        if let Ok(cur) = lore_backend::current_revision(project_root) {
            if cur != rollback_point {
                let _ = lore_backend::branch_reset(project_root, &rollback_point);
                let _ = restore_backup_at(project_root, &rollback_backup_new_dir(project_root), &[]);
            }
        }
    }
    result?;
    eprintln!("[restore] 完成");
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
    eprintln!("[restore] new_backup={}", new_backup);
    // 检出新增文件（HEAD 树无 → untracked，backup_dirty_to 不处理）会阻止 branch reset
    // （lore 对 untracked 有本地内容也拒绝同步）→ 丢弃 = 物理删除（仅 change_files 内的检出新增）
    for f in changed_files(project_root, hidden)? {
        if f.state == LoreState::Untracked && marker.change_files.contains(&f.path) {
            eprintln!("[restore] 丢弃检出新增文件: {}", f.path);
            let _ = std::fs::remove_file(project_root.join(&f.path));
        }
    }
    let t_reset = std::time::Instant::now();
    reset_loreignore_to(project_root, &marker.head_before)?;
    lore_backend::branch_reset(project_root, &marker.head_before)?;
    eprintln!("[restore] branch_reset: {}ms", t_reset.elapsed().as_millis());
    // 统一写回：branch reset 不处理忽略文件（残留回退点内容）→ 丢弃项（exclude_new）中
    // head_before 树含的文件从 head_before 写回覆盖（含忽略文件）；head_before 树无的
    // （检出新增）已在上方物理删除；留下项（exclude_new 不含）保留用户内容
    let exclude_set: std::collections::HashSet<&str> =
        exclude_new.iter().map(String::as_str).collect();
    let head_files: std::collections::HashSet<String> =
        dump_tree_files(project_root, &marker.head_before)?.into_iter().collect();
    for p in &marker.change_files {
        if head_files.contains(p) && exclude_set.contains(p.as_str()) {
            let _ = write_back(project_root, p, &marker.head_before);
        }
    }
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

/// web 产物版本号（部署版本 hint）：`.keinestamp/web-version` 本地计数器（产物重建时自增）。
/// `.keinestamp/` 整体隔离后 web 不进 lore 仓库，版本不再来自 kind=web 历史计数。
pub fn web_revision_count(project_root: &Path) -> Result<u64> {
    let path = project_root.join(crate::hidden::PRIVATE_DIR).join("web-version");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ok(0);
    };
    Ok(content.trim().parse::<u64>().unwrap_or(0))
}

/// web 产物重建后自增版本号（原子写：temp + rename）。
pub fn bump_web_version(project_root: &Path) -> Result<u64> {
    let next = web_revision_count(project_root)? + 1;
    let dir = project_root.join(crate::hidden::PRIVATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("web-version");
    let tmp = dir.join(format!("web-version.tmp{}", std::process::id()));
    std::fs::write(&tmp, next.to_string())?;
    std::fs::rename(&tmp, &path)?;
    Ok(next)
}

/// 导出 entry 全部文件到指定目录（dump 解析 + 逐文件导出）
pub fn export_entry(project_root: &Path, revision: &str, dest: &Path) -> Result<()> {
    lore_backend::export_files(project_root, revision, dest)
}

/// 备份未提交改动到指定目录（暂存语义，v1 stash）：
/// - Modified tracked 文件：复制到备份目录（保留相对路径）→ `file reset` 还原到 HEAD
/// - Deleted tracked 文件：记清单（写入 `backup-meta.json` 持久化，供跨进程恢复）→ `file reset` 恢复
/// - untracked 文件：rename 移入备份目录（lore branch reset 对 untracked 有本地内容也拒绝同步，
///   必须移出工作区；恢复时随备份整体取回，顶替记录时随 clear_rollback 丢弃）
/// 返回是否产生了备份（无改动 → false）。
fn backup_dirty_to(project_root: &Path, hidden: &[String], dir: &Path) -> Result<bool> {
    let files = changed_files(project_root, hidden)?;
    let mut reset_paths: Vec<String> = Vec::new();
    let mut deleted = Vec::new();
    let mut backup_files = Vec::new();
    let mut untracked = Vec::new();
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
            LoreState::Untracked => untracked.push(f.path.clone()),
            LoreState::Added => bail!("存在已暂存的改动（staged state），请先处理后回退"),
            _ => {}
        }
    }
    if reset_paths.is_empty() && untracked.is_empty() {
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
    for p in &untracked {
        let src = project_root.join(p);
        if src.exists() {
            let dst = dir.join(p);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&src, &dst)?;
        }
    }
    if !reset_paths.is_empty() {
        let mut args: Vec<&str> = vec!["file", "reset"];
        for p in &reset_paths {
            args.push(p);
        }
        lore_backend::run_lore(project_root, &args)?;
    }
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
    use crate::lore_backend::{self, create_repo, revision_history};
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
    fn test_changed_files_filters_dir_lines() {
        // lore 把目录作为一等条目：空目录/未跟踪目录会输出目录行（`A out/`）→ 变更面板
        // 会出现"幻影文件" → changed_files 必须过滤目录行（文件行足以呈现树节点）
        let root = tmp_root("cfdir");
        create_repo(&root).unwrap();
        // 空目录（lore status 报 `A out/`）
        std::fs::create_dir_all(root.join("out")).unwrap();
        // 未跟踪目录含文件（lore status 报 `A models/` + `A models/x.blend`）
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(root.join("models/x.blend"), "1").unwrap();
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().all(|f| !f.path.ends_with('/')),
            "目录行应被过滤: {:?}",
            files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );
        assert!(
            files.iter().any(|f| f.path == "models/x.blend"),
            "目录下文件行应保留"
        );
    }

    #[test]
    fn test_discard_file_changes_modified_and_deleted() {
        // 取消更改（git restore 语义）：M 覆盖回最新 HEAD / D 从 HEAD 恢复；目录行/空路径拒绝
        let root = tmp_root("discard");
        create_repo(&root).unwrap();
        let hidden = vec![".keinestamp/".to_string()];
        // 两次提交：HEAD 是第二版，reset 应还原到最新 HEAD
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        lore_backend::commit_files(&root, "c1", &[], &["a.txt".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        lore_backend::commit_files(&root, "c2", &[], &["a.txt".to_string()]).unwrap();
        // M：修改后取消 → 内容还原到 v2 + status 干净
        std::fs::write(root.join("a.txt"), "v3-modified").unwrap();
        discard_file_changes(&root, "a.txt").unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        assert!(
            changed_files(&root, &hidden)
                .unwrap()
                .iter()
                .all(|f| f.path != "a.txt")
        );
        // D：删除后取消 → 恢复
        std::fs::remove_file(root.join("a.txt")).unwrap();
        discard_file_changes(&root, "a.txt").unwrap();
        assert!(root.join("a.txt").exists());
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        // 嵌套路径 + 空格/中文文件名（覆盖用户项目 `refer/ChatGPT Image ....png` 特征）
        let space_name = "refer/ChatGPT Image 2026年7月29日 03_28_37.png";
        let nested = root.join(space_name);
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, "img1").unwrap();
        lore_backend::commit_files(&root, "c3", &[], &[space_name.to_string()]).unwrap();
        std::fs::write(&nested, "img2-modified").unwrap();
        discard_file_changes(&root, space_name).unwrap();
        assert_eq!(std::fs::read_to_string(&nested).unwrap(), "img1");
        // D + 父目录也删了：file reset 不建父目录 → create_dir_all 兜底恢复
        std::fs::remove_file(&nested).unwrap();
        std::fs::remove_dir_all(nested.parent().unwrap()).unwrap();
        discard_file_changes(&root, space_name).unwrap();
        assert_eq!(std::fs::read_to_string(&nested).unwrap(), "img1");
        // 防御：目录行/空路径拒绝
        assert!(discard_file_changes(&root, "out/").is_err());
        assert!(discard_file_changes(&root, "").is_err());
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
    fn test_web_version_local_counter() {
        // .keinestamp/ 隔离后 web 版本 = 本地计数器（bump 自增，读回一致）
        let root = tmp_root("webcnt");
        create_repo(&root).unwrap();
        assert_eq!(web_revision_count(&root).unwrap(), 0);
        assert_eq!(bump_web_version(&root).unwrap(), 1);
        assert_eq!(bump_web_version(&root).unwrap(), 2);
        assert_eq!(web_revision_count(&root).unwrap(), 2);
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
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
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
        assert!(!root.join("new.txt").exists(), "untracked 文件备份移出工作区（branch reset 要求）");
        assert_eq!(
            std::fs::read_to_string(root.join(".keinestamp").join("rollback-backup").join("new.txt")).unwrap(),
            "n",
            "untracked 备份在 rollback-backup"
        );
        assert!(rollback_state(&root).unwrap().active);
        // 撤销回退：HEAD 回 r2 + 暂存 dirty 取回（无冲突自动带回）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "B-DIRTY", "暂存 dirty 取回");
        assert_eq!(std::fs::read_to_string(root.join("new.txt")).unwrap(), "n", "untracked 文件取回");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_checked_out_with_stash_no_conflict() {
        // 回退前 dirty（备份到 rollback-backup）+ 纯检出 → 无冲突：
        // 恢复直接执行（检出默认丢弃、回退前改动自动取回）
        let root = tmp_root("rbconf");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退前 dirty a.txt（A-DIRTY，备份进 rollback-backup）
        std::fs::write(root.join("a.txt"), "A-DIRTY").unwrap();
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // 检出 r2 的 a.txt=v2（覆盖 dirty，dirty 已暂存）
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        // 纯检出（用户未改）∩ 回退前暂存 → 无冲突
        let conflicts = rollback_conflicts(&root).unwrap();
        assert!(conflicts.is_empty(), "纯检出 + 回退前 dirty 不应冲突: {:?}", conflicts);
        // 直接恢复（无需确认）→ a.txt = A-DIRTY（回退前改动取回）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "A-DIRTY", "回退前改动取回");
    }

    #[test]
    fn test_rollback_conflict_resolve() {
        // 回退后修改检出文件（真修改）∩ 回退前暂存 → 冲突 → resolve
        let root = tmp_root("rbconf2");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退前 dirty a.txt（A-DIRTY，备份进 rollback-backup）
        std::fs::write(root.join("a.txt"), "A-DIRTY").unwrap();
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // 回退后修改检出文件（真修改）
        std::fs::write(root.join("a.txt"), "V2-EDITED").unwrap();
        // 冲突检测：真修改（a.txt V2-EDITED）∩ 暂存（a.txt A-DIRTY）
        let conflicts = rollback_conflicts(&root).unwrap();
        assert_eq!(conflicts, vec!["a.txt".to_string()]);
        // undo 直接调用应报冲突
        assert!(undo_rollback(&root, &vec![".keinestamp/".to_string()], None).is_err());
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
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_first_entry_resets_to_self() {
        let root = tmp_root("rbfirst");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首条 r1 回退（无父，旧项目无 init 提交）：reset 自身 + 补 init 提交（.loreignore only）→
        // 工作区 = r1 内容，文件变 untracked（changes 区可重新勾选/修改）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        let init_id = lore_backend::current_revision(&root).unwrap();
        assert_ne!(init_id, r1, "HEAD = 补的 init 提交（非 r1 自身）");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1", "工作区 = r1 内容");
        assert!(rollback_state(&root).unwrap().active);
        // status 显示 a.txt untracked（可重新勾选）
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().any(|f| f.path == "a.txt" && f.state == LoreState::Untracked),
            "a.txt 应为 untracked"
        );
        // marker：rollback_point = 补的 init；superseded_point = r1；change_files = [a.txt]
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.rollback_point, init_id);
        assert_eq!(marker.superseded_point, r1);
        assert_eq!(marker.change_files, vec!["a.txt".to_string()]);
        assert!(marker.deleted_files.is_empty());

        // 撤销回退：HEAD 回 r3 + 内容还原
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3, "HEAD 恢复为 r3");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r2;
    }

    #[test]
    fn test_rollback_first_entry_supersede() {
        let root = tmp_root("rbfirstsup");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首条回退（无父 → 补 init 提交）后修改 → 记录顶替（新 revision 父 = 补的 init）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        let init_id = lore_backend::current_revision(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v4").unwrap();
        let r4 = record_after_rollback(&root, "r4", None, &vec![".keinestamp/".to_string()]).unwrap();
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist[0].message, "r4");
        assert_eq!(hist.len(), 3, "主线 = r4 + 补的 init + r1（r2/r3 顶替出主线）");
        assert_eq!(hist[1].metadata.get("kind").map(String::as_str), Some("init"), "中间是补的 init 提交");
        // r4 真实父 = 补的 init；缓存推导跳过 init 语义层 → parent = r1（回退 r4 时 reset r1 +
        // 检出 r4 改动为 M，语义正确——init 仅作首条回退的空基底）
        assert_eq!(lore_backend::revision_parent(&root, &r4).unwrap().as_deref(), Some(r1.as_str()), "r4 缓存父 = r1");
        assert!(!rollback_state(&root).unwrap().active);
        // timeline 中 r2/r3 标记 superseded（折叠；r1 仍在主线——补的 init 继承其内容）
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
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r4);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v4");
        let _ = r1;
    }

    #[test]
    fn test_rollback_with_loreignore_change() {
        // .loreignore 是自动记录文件（每次记录 --force stage 提交）：
        // 记录间修改它 → 回退该记录（change_files 过滤 .loreignore，不检出）→
        // 恢复 branch_reset(head_before) 前须把 .loreignore 还原到目标树版本（否则 dirty 校验卡）
        let root = tmp_root("rblgig");
        create_repo(&root).unwrap();
        std::fs::write(root.join(".loreignore"), "*.tmp\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 修改 .loreignore（加规则）→ r2 记录（自动提交新 .loreignore）
        std::fs::write(root.join(".loreignore"), "*.tmp\n*.bak\n").unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退 r2 → 目标 r1；.loreignore 不应出现在 change_files（过滤）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert!(
            !marker.change_files.iter().any(|p| p == crate::hidden::LOREIGNORE),
            "change_files 不应含 .loreignore: {:?}",
            marker.change_files
        );
        // 恢复：branch_reset(head_before=r2) 前 reset_loreignore_to 还原 .loreignore → 成功
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD 恢复为 r2");
        let _ = r1;
    }

    #[test]
    fn test_rollback_consecutive_discard_drops_untracked() {
        // 连续回退确认丢弃：untracked（回退期间新建）也物理删除（与弹窗文案一致）
        let root = tmp_root("rbconsuntracked");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 首次回退 r2 → 回退期间新建 untracked（不在树，不算"真修改"）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("new.txt"), "n").unwrap();
        // 连续回退 r3：untracked 不算修改 → 不弹确认，直接丢弃（discard_dirty 物理删除）
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap();
        assert!(!root.join("new.txt").exists(), "丢弃后 untracked 应删除");
        let _ = r1;
    }

    #[test]
    fn test_undo_rollback_deleted_not_confirm() {
        // 回退删除标记（deleted_files）属检出的一部分：无真修改时不触发恢复确认，
        // 恢复直接执行（branch_reset(head_before) 自动删除该文件）
        let root = tmp_root("rbdelconfirm");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // r2 删除 a.txt（D 提交）
        std::fs::remove_file(root.join("a.txt")).unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退 r2 → 目标 r1（a.txt 存在 = 回退删除检出，deleted_files=[a.txt]）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.deleted_files, vec!["a.txt".to_string()], "回退删除标记");
        // 无真修改 → 恢复直接执行（不报"需要确认"）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD 恢复为 r2");
        assert!(!root.join("a.txt").exists(), "恢复后 a.txt 不存在（head_before 树无）");
    }

    #[test]
    fn test_undo_rollback_confirm_modified() {
        // 恢复确认：changes 区有已修改内容（非纯检出）→ 未确认报错；确认丢弃后恢复成功
        let root = tmp_root("rbundoc");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 回退 r2 → 检出 a.txt（v2）；用户修改检出内容 → 真修改
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("a.txt"), "V2-EDITED").unwrap();
        // 未确认（None）→ 报"恢复需要确认"
        let err = undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap_err();
        assert!(err.to_string().contains("恢复需要确认"), "err: {err}");
        // 确认丢弃（Some(true)）→ 恢复成功，a.txt = r2 树版（v2，修改丢弃）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], Some(true)).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2", "丢弃后 = 回退前版本");
        let _ = r1;
    }

    #[test]
    fn test_undo_rollback_keep_modified() {
        // 恢复时选择"留下修改"（Some(false)）：已修改的检出内容取回（恢复后 = 修改版）
        let root = tmp_root("rbkeep");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        std::fs::write(root.join("a.txt"), "V2-EDITED").unwrap();
        // 留下（Some(false)）→ 恢复后 a.txt = V2-EDITED（修改带回）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], Some(false)).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "V2-EDITED", "留下修改 = 修改版带回");
        let _ = r1;
    }

    #[test]
    fn test_delete_entry_survives_rebuild() {
        // 删除条目（tombstone）→ 后续记录（rebuild_cache）不把被删条目拉回
        let root = tmp_root("rbdelkeep");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 删除 r2 条目（tombstone）
        let tl_path = crate::timeline::timeline_cache_path(&root);
        crate::timeline::remove_entry(&tl_path, &r2).unwrap();
        let tl = crate::timeline::read_timeline(&tl_path).unwrap();
        assert_eq!(tl.entries.len(), 1, "r2 已删除");
        assert!(tl.removed_entries.contains(&r2), "tombstone 记录");

        // 新记录触发 rebuild_cache → r2 不应被拉回
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        record(&root, "r3", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let tl = crate::timeline::read_timeline(&tl_path).unwrap();
        let msgs: Vec<&str> = tl.entries.iter().map(|e| e.message.as_str()).collect();
        assert!(!msgs.contains(&"r2"), "被删条目不应被 rebuild 拉回: {:?}", msgs);
    }

    #[test]
    fn test_record_selected_subset_with_chinese_filenames() {
        // 中文/空格文件名场景：勾选子集 → 只提交勾选（现象 1 复现验证）
        let root = tmp_root("rbcn");
        create_repo(&root).unwrap();
        std::fs::write(root.join(".loreignore"), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("b.txt"), "b").unwrap();
        std::fs::create_dir_all(root.join("refer").join("test")).unwrap();
        std::fs::write(root.join("refer/test/ChatGPT Image 2026年7月24日 17_23_04.png"), "img1").unwrap();
        std::fs::write(root.join("refer/test/ChatGPT Image 2026年7月24日 19_45_30.png"), "img2").unwrap();
        // 勾选 a/b（不选中文文件）
        let files = vec!["a.txt".to_string(), "b.txt".to_string()];
        record(&root, "user1", None, Some(&files), &vec![".keinestamp/".to_string()]).unwrap();
        // 断言：提交只含 a/b + .loreignore（不含 refer 中文文件）
        let hist = lore_backend::revision_history(&root).unwrap();
        let out = lore_backend::run_lore(&root, &["repository", "dump", "--revision", &hist[0].id]).unwrap();
        assert!(out.contains("a.txt"), "应提交 a.txt");
        assert!(out.contains("b.txt"), "应提交 b.txt");
        assert!(!out.contains("refer/"), "不应提交 refer 文件: {}", out);
    }

    #[test]
    fn test_record_filters_directory_paths() {
        // files 含目录路径（`lore stage <目录>` = 目录下全部 dirty 文件——前端勾选 UI 的
        // 目录残留会让未勾选文件被提交）→ 只提交勾选文件，目录路径丢弃
        let root = tmp_root("dirpaths");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::create_dir_all(root.join("modeling")).unwrap();
        std::fs::write(root.join("modeling/model.blend"), "m1").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(
            &root,
            "r1",
            None,
            Some(&["a.txt".to_string(), "modeling/".to_string()]),
            &vec![".keinestamp/".to_string()],
        )
        .unwrap();
        let out = lore_backend::run_lore(&root, &["repository", "dump", "--revision", &r1]).unwrap();
        assert!(out.contains("a.txt"), "提交含 a.txt");
        assert!(!out.contains("modeling"), "目录下文件未被提交: {}", out);
        // 未勾选文件仍在工作区（untracked）
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().any(|f| f.path == "modeling/model.blend" && f.state == LoreState::Untracked),
            "modeling/model.blend 仍 untracked"
        );
    }

    #[test]
    fn test_staged_residue_does_not_pollute_record() {
        // 残留 staged（stage 文件但不提交）→ record 勾选子集 → 只提交勾选（残留不带入）
        let root = tmp_root("stagedres");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "v2").unwrap();
        // 制造残留：stage a.txt 但不提交（模拟异常残留）
        lore_backend::run_lore(&root, &["stage", "a.txt"]).unwrap();
        // record 只勾选 b.txt → 提交只含 b.txt + .loreignore（残留 a.txt 不被提交）
        let r1 = record(
            &root,
            "r1",
            None,
            Some(&["b.txt".to_string()]),
            &vec![".keinestamp/".to_string()],
        )
        .unwrap();
        let out = lore_backend::run_lore(&root, &["repository", "dump", "--revision", &r1]).unwrap();
        let paths: Vec<String> = out
            .lines()
            .filter(|l| l.contains(" addr "))
            .filter_map(|l| l.split_whitespace().next().map(|p| p.to_string()))
            .filter(|p| !p.ends_with('/'))
            .collect();
        assert!(paths.contains(&"b.txt".to_string()), "提交含 b.txt");
        assert!(paths.contains(&".loreignore".to_string()), "提交含 .loreignore");
        assert!(!paths.contains(&"a.txt".to_string()), "残留 a.txt 未被提交");
        // 残留已清理：a.txt 仍为 untracked（未提交）
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(files.iter().any(|f| f.path == "a.txt" && f.state == LoreState::Untracked), "a.txt 仍 untracked");
    }

    #[test]
    fn test_first_rollback_missing_files_on_disk() {
        // 树有磁盘无（记录后文件被删/改名）→ remove NotFound 视为已删 → 回退成功 + 文件 untracked
        let root = tmp_root("rbfirstmiss");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "v2").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 模拟磁盘文件被删（tracked 删除）+ 新 untracked 文件（模拟改名）
        std::fs::remove_file(root.join("a.txt")).unwrap();
        std::fs::write(root.join("c.txt"), "new").unwrap();
        // 回退首条（无父）→ 补 init → 全部文件写回 untracked（回退成功，不丢文件）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1", "a.txt 写回");
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "v2", "b.txt 写回");
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().any(|f| f.path == "a.txt" && f.state == LoreState::Untracked),
            "a.txt untracked"
        );
        assert!(
            files.iter().any(|f| f.path == "b.txt" && f.state == LoreState::Untracked),
            "b.txt untracked"
        );
        // 恢复：HEAD 回 r1 + 文件还原
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1, "HEAD 恢复为 r1");
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_to_init_creates_parent_dirs() {
        // 回退到 init（树 = 仅 .loreignore）后工作区目录被清空，检出嵌套路径文件
        // 必须先建父目录（lore file reset 不创建，实测 os error 2）
        let root = tmp_root("rbnestdirs");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::create_dir_all(root.join("refer")).unwrap();
        std::fs::create_dir_all(root.join("modeling/deep")).unwrap();
        std::fs::write(root.join("refer/back.png"), "r1").unwrap();
        std::fs::write(root.join("modeling/deep/a.blend"), "m1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        assert_eq!(
            lore_backend::revision_parent(&root, &r1).unwrap().as_deref(),
            None,
            "无 init 提交（直接 create_repo）——走 None 分支补 init"
        );
        // 回退首条：None 分支（reset 自身 + 补 init + 写回）——嵌套目录文件写回需建父目录
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("refer/back.png")).unwrap(),
            "r1",
            "refer/back.png 写回（父目录已建）"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("modeling/deep/a.blend")).unwrap(),
            "m1",
            "modeling/deep/a.blend 写回（嵌套父目录已建）"
        );
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().any(|f| f.path == "refer/back.png" && f.state == LoreState::Untracked),
            "refer/back.png untracked"
        );
        // 恢复：HEAD 回 r1，目录文件还原
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1);
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_some_branch_nested_dirs() {
        // 用户场景：init 提交 + 记录嵌套目录文件 → 回退（Some 分支 reset 到 init）→
        // branch reset 清空工作区目录 → file reset 检出嵌套路径须先建父目录
        let root = tmp_root("rbsomenest");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        crate::lore_backend::init_commit(&root).unwrap();
        std::fs::create_dir_all(root.join("refer")).unwrap();
        std::fs::write(root.join("refer/back.png"), "r1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 回退到 init（Some 分支）→ 检出 refer/back.png（refer/ 已被 reset 清空）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("refer/back.png")).unwrap(),
            "r1",
            "检出成功（父目录已建）"
        );
        let files = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(
            files.iter().any(|f| f.path == "refer/back.png" && f.state == LoreState::Untracked),
            "refer/back.png untracked"
        );
        // 恢复：HEAD 回 r1
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1);
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_untracked_not_modified() {
        // 回退后：untracked 残留/新建文件（不在树）不算"真修改"——丢弃/恢复弹窗不列出；
        // 修改过检出内容（hash≠）仍算
        let root = tmp_root("rbuntracked");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 回退 r2（父 = r1）→ 检出 a.txt（v2）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        // untracked 残留（不在树）+ 真新文件（untracked）
        std::fs::write(root.join("leftover.txt"), "残留").unwrap();
        std::fs::write(root.join("newfile.txt"), "新文件").unwrap();
        let modified = rollback_modified_files(&root).unwrap();
        assert!(!modified.contains(&"leftover.txt".to_string()), "untracked 残留不算修改: {modified:?}");
        assert!(!modified.contains(&"newfile.txt".to_string()), "untracked 新文件不算修改: {modified:?}");
        // 修改检出内容（a.txt 改为 v3）→ 算修改
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let modified2 = rollback_modified_files(&root).unwrap();
        assert!(modified2.contains(&"a.txt".to_string()), "修改检出内容算修改: {modified2:?}");
        // 恢复：untracked 残留不弹确认直接恢复（modified 含 a.txt 时才确认——清回 v2 后恢复）
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2);
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_with_space_filenames() {
        // 含空格文件名（dump 行 `<path> id ...`——split_whitespace 截断路径）→
        // " id " 分隔解析完整路径，回退写回成功
        let root = tmp_root("rbspace");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::create_dir_all(root.join("refer/test")).unwrap();
        std::fs::write(root.join("refer/芙姆米新1皮三视图 拷贝.png"), "v1").unwrap();
        std::fs::write(root.join("refer/test/ChatGPT Image 2026年7月24日 17_23_04.png"), "img1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 首条回退（None 分支：补 init + 写回）——含空格文件完整写回
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("refer/芙姆米新1皮三视图 拷贝.png")).unwrap(),
            "v1",
            "含空格文件写回（完整路径）"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("refer/test/ChatGPT Image 2026年7月24日 17_23_04.png")).unwrap(),
            "img1",
            "深层含空格文件写回"
        );
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert!(
            marker
                .change_files
                .contains(&"refer/芙姆米新1皮三视图 拷贝.png".to_string()),
            "change_files 含完整路径"
        );
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1);
    }

    #[test]
    fn test_rollback_ignored_file_checked_out() {
        // 忽略文件回退：忽略前提交的文件，忽略后工作区被改（v2 未提交）→ 回退所选版本时
        // 忽略文件也被写回为该版本树内容（v1）——统一检出语义，不做忽略特例
        let root = tmp_root("rbignored");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 忽略 a.txt + 工作区改为 v2（被忽略不 stage）+ 新增 b.txt（有改动可记录）
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\na.txt\n").unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        std::fs::write(root.join("b.txt"), "b1").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // r2 树保留 a.txt（v1——忽略只影响后续 stage，不移除树内旧版本）
        let out = lore_backend::run_lore(&root, &["repository", "dump", "--revision", &r2]).unwrap();
        assert!(out.contains("a.txt"), "r2 树保留 a.txt");
        // 回退 r2（Some 分支，父 = r1）→ 检出 = r2 树全部（含忽略文件 a.txt → 写回 v1）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "v1",
            "忽略文件检出为 r2 树版本（v1，覆盖工作区 v2）"
        );
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "b1", "b.txt 检出");
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert!(marker.change_files.contains(&"a.txt".to_string()), "忽略文件算检出文件");
        assert!(marker.change_files.contains(&"b.txt".to_string()), "b.txt 算检出文件");
        // 恢复：HEAD 回 r2 + 内容还原（忽略文件 a.txt 写回 head_before 版本 = v1）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD 恢复为 r2");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_init_commit_and_first_rollback_untracked() {
        // init 提交（.loreignore，kind=init）→ 首条用户记录有父 →
        // 回退首条 = 回到 init（用户文件检出为 untracked，changes 区可重新勾选）
        let root = tmp_root("rbinit");
        create_repo(&root).unwrap();
        std::fs::write(root.join(crate::hidden::LOREIGNORE), ".keinestamp/\n").unwrap();
        crate::lore_backend::init_commit(&root).unwrap();
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1, "init 提交存在");
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("init"));

        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "v2").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        // 首条用户记录父 = init
        let parent = lore_backend::revision_parent(&root, &r1).unwrap().unwrap();
        assert_eq!(parent, hist[0].id, "首条用户记录父 = init 提交");

        // 回退首条 → 目标 = init → a/b 检出为 untracked（changes 区可显示）
        rollback(&root, &r1, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), hist[0].id, "HEAD = init");
        let marker = crate::timeline::read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.rollback_point, hist[0].id);
        assert!(marker.change_files.contains(&"a.txt".to_string()), "change_files 含 a.txt");
        assert!(marker.change_files.contains(&"b.txt".to_string()), "change_files 含 b.txt");
        // 工作区文件存在（检出）且是 untracked（HEAD=init 树无）
        assert!(root.join("a.txt").exists() && root.join("b.txt").exists(), "检出文件在工作区");
        let lines = changed_files(&root, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(lines.iter().any(|f| f.path == "a.txt" && f.state == LoreState::Untracked), "a.txt 应为 untracked");
        // 恢复：HEAD 回 r1，a/b 还原
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1, "HEAD 恢复为 r1");
        assert!(!rollback_state(&root).unwrap().active);
    }

    #[test]
    fn test_rollback_failure_restores_backup() {
        // 回退失败（目标 revision 不可用）时：已备份的 dirty 应还原回工作区（否则文件"消失"）
        let root = tmp_root("rbfail");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        std::fs::write(root.join("b.txt"), "b1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let _ = (r1, r2);

        // dirty：b.txt 修改 + untracked 新文件
        std::fs::write(root.join("b.txt"), "B-DIRTY").unwrap();
        std::fs::write(root.join("new.txt"), "n").unwrap();
        // 目标 revision 不存在 → revision info 失败 → 回退失败（备份已执行）
        let err = rollback(&root, "deadbeefdeadbeefdeadbeefdeadbeef", &vec![".keinestamp/".to_string()], false).unwrap_err();
        assert!(err.to_string().contains("failed"), "err: {err}");
        assert!(!rollback_state(&root).unwrap().active, "失败不应留下回退态");
        // dirty 还原回工作区（备份取回），数据不丢
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "B-DIRTY", "dirty 文件还原");
        assert_eq!(std::fs::read_to_string(root.join("new.txt")).unwrap(), "n", "untracked 文件还原");
        assert!(!rollback_backup_dir(&root).exists(), "失败后备份目录已还原并清理");
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
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
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
        let _ = r1;
        let _ = r3;

        // 回退 r2 → 纯检出（a.txt = v2）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(rollback_modified_files(&root).unwrap(), Vec::<String>::new(), "纯检出不算修改");
        // 连续回退 r3：只显示"已检出"不阻碍回退 → 不弹窗自动丢弃检出继续
        rollback(&root, &r3, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD = r3 的父 r2");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3", "r3 改动检出");
        // 恢复：回到最初（r3）干净
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r3);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");
        assert!(!rollback_state(&root).unwrap().active);
        let _ = r1;
    }

    #[test]
    fn test_rollback_undo_with_added_file() {
        // 回退的记录含新增文件（HEAD 树无 → untracked）→ undo 应成功（丢弃检出新增）
        let root = tmp_root("rbadd");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        let r1 = record(&root, "r1", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        std::fs::write(root.join("new.txt"), "new-data").unwrap();
        let r2 = record(&root, "r2", None, None, &vec![".keinestamp/".to_string()]).unwrap();

        // 回退 r2（新增 new.txt 检出 → untracked）
        rollback(&root, &r2, &vec![".keinestamp/".to_string()], false).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r1);
        assert!(root.join("new.txt").exists(), "检出新增文件在工作区");
        // 恢复：应成功（丢弃 untracked 检出新增）
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
        assert_eq!(lore_backend::current_revision(&root).unwrap(), r2, "HEAD 恢复为 r2");
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "new-data",
            "r2 树含 new.txt"
        );
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
        undo_rollback(&root, &vec![".keinestamp/".to_string()], None).unwrap();
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
        let _ = r2;
    }
}
