use std::collections::HashMap;
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
use anyhow::{bail, Context, Result};

pub const LORE_BIN: &str = "C:\\Users\\para\\AppData\\Local\\lore\\bin\\lore.exe";
pub const LORE_SERVER_URL: &str = "lore://127.0.0.1:41337";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoreState {
    Added,
    Modified,
    Deleted,
    Untracked,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoreStatusLine {
    pub path: String,
    pub state: LoreState,
}

#[derive(Debug, Clone)]
pub struct LoreRevision {
    pub number: u32,
    pub id: String,
    pub message: String,
    pub date: String,
    pub metadata: HashMap<String, String>,
}

pub fn lore_bin() -> String {
    std::env::var("KEINESTAMP_LORE_BIN").unwrap_or_else(|_| LORE_BIN.to_string())
}

pub fn run_lore(project_root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(lore_bin())
        .args(args)
        .current_dir(project_root)
        .output()
        .context("failed to spawn lore")?;
    if !out.status.success() {
        bail!("lore {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn repo_exists(project_root: &Path) -> bool {
    project_root.join(".lore").is_dir()
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn create_repo(project_root: &Path) -> Result<()> {
    let name = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let url = format!("{}/{}", LORE_SERVER_URL, sanitize_name(name));
    run_lore(project_root, &["repository", "create", &url])?;
    Ok(())
}

pub fn commit(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String> {
    run_lore(project_root, &["stage", "--scan", "."])?;
    commit_staged(project_root, message, metadata)
}

pub fn commit_files(project_root: &Path, message: &str, metadata: &[(&str, &str)], paths: &[String]) -> Result<String> {
    if !paths.is_empty() {
        let mut args: Vec<&str> = vec!["stage"];
        for p in paths {
            args.push(p);
        }
        run_lore(project_root, &args)?;
    }
    commit_staged(project_root, message, metadata)
}

/// 提交 web/ 目录全部产物（index.html、timeline.json、previews_glb 等），metadata kind=web。
/// web/ 不存在、为空或本次无改动时跳过提交返回 Ok（部署不应因无 web 产物而报错）。
/// 命令形态（实测）：`lore stage --scan web/` 只 stage web/ 下 dirty 文件（`lore stage web/`
/// 目录不 --scan 只 stage 已 dirty 文件）；无改动时 stage 输出 "No changes staged" 且 exit 0，
/// 故先经 `status --scan` 判断 changed 非空再 stage/commit，避免空提交。
pub fn commit_web(project_root: &Path) -> Result<()> {
    let web_dir = project_root.join("web");
    if !web_dir.is_dir() || std::fs::read_dir(&web_dir)?.next().is_none() {
        return Ok(());
    }
    let changed: Vec<String> = status_lines(project_root)?
        .into_iter()
        .filter(|f| f.path.starts_with(crate::hidden::WEB_DIR))
        .map(|f| f.path)
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    run_lore(project_root, &["stage", "--scan", "web/"])?;
    commit_staged(project_root, "web update", &[("kind", "web")])?;
    Ok(())
}

fn commit_staged(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String> {
    if !metadata.is_empty() {
        let _ = run_lore(project_root, &["revision", "metadata", "clear"]);
        for (k, v) in metadata {
            run_lore(project_root, &["revision", "metadata", "set", k, v])?;
        }
    }
    let out = run_lore(project_root, &["commit", message])?;
    parse_commit_signature(&out).context("commit 输出解析失败：未找到 Signature 行")
}

fn parse_commit_signature(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("Signature")
            .map(|s| s.trim_start_matches([' ', ':']).trim().to_string())
    })
}

const META_SKIP: [&str; 7] = ["Branch", "Date", "Repository", "Revision", "Signature", "Parent", "Tree"];

pub fn revision_metadata(project_root: &Path, revision: &str) -> Result<HashMap<String, String>> {
    let out = run_lore(project_root, &["revision", "metadata", "get", "--revision", revision])?;
    let mut map = HashMap::new();
    for line in out.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let Some((k, v)) = l.split_once(':') else { continue };
        let key = k.trim();
        if META_SKIP.contains(&key) {
            continue;
        }
        map.insert(key.to_string(), v.trim().to_string());
    }
    Ok(map)
}

/// 全部 revision 历史（最新在前）。解析 `lore history`（非 --oneline）输出块：
/// 每块含 Revision/Signature/Date/message 行，message 为 Date 后缩进行。
pub fn revision_history(project_root: &Path) -> Result<Vec<LoreRevision>> {
    let out = run_lore(project_root, &["history"])?;
    let mut revs: Vec<(u32, String, String, Vec<String>)> = Vec::new();
    let mut cur: Option<(u32, String, String, Vec<String>)> = None;
    let mut in_message = false;
    for line in out.lines() {
        let t = line.trim_end();
        if let Some(rest) = t.strip_prefix("Revision") {
            if let Some(prev) = cur.take() {
                revs.push(prev);
            }
            in_message = false;
            if let Ok(number) = rest.trim_start_matches([' ', ':']).trim().parse::<u32>() {
                cur = Some((number, String::new(), String::new(), Vec::new()));
            }
        } else if let Some(c) = cur.as_mut() {
            if let Some(rest) = t.strip_prefix("Signature") {
                c.1 = rest.trim_start_matches([' ', ':']).trim().to_string();
            } else if let Some(rest) = t.strip_prefix("Date") {
                c.2 = rest.trim_start_matches([' ', ':']).trim().to_string();
                in_message = true;
            } else if in_message {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    c.3.push(trimmed.to_string());
                }
            }
        }
    }
    if let Some(prev) = cur.take() {
        revs.push(prev);
    }
    let mut out_revs = Vec::new();
    for (number, id, date, msg_lines) in revs {
        let metadata = revision_metadata(project_root, &id).unwrap_or_default();
        out_revs.push(LoreRevision {
            number,
            id,
            message: msg_lines.join("\n"),
            date,
            metadata,
        });
    }
    Ok(out_revs)
}

/// 该 revision 的父 revision 签名（`revision info` 的 Parent 行；首条 revision 无父返回 None）
pub fn revision_parent(project_root: &Path, revision: &str) -> Result<Option<String>> {
    let out = run_lore(project_root, &["revision", "info", revision])?;
    Ok(out.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("Parent")
            .map(|s| s.trim_start_matches([' ', ':']).trim().to_string())
    }))
}

/// 目录级导出：`repository dump` 解析文件树 → 逐文件 `file write` 到 dest（v1 export_entry 对应）。
/// dump 文件行含 ` addr `，目录行以 `/` 结尾（含 ` child N`），仅解析前者。
pub fn export_files(project_root: &Path, revision: &str, dest: &Path) -> Result<()> {
    let out = run_lore(project_root, &["repository", "dump", "--revision", revision])?;
    let mut written = 0usize;
    for line in out.lines() {
        if !line.contains(" addr ") {
            continue;
        }
        let Some(path) = line.split_whitespace().next() else { continue };
        if path.ends_with('/') {
            continue;
        }
        let target = dest.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let out_path = target.to_string_lossy();
        run_lore(
            project_root,
            &["file", "write", "--path", path, "--revision", revision, "--output", out_path.as_ref()],
        )?;
        written += 1;
    }
    if written == 0 {
        bail!("dump 中未解析到任何文件（revision {}）", revision);
    }
    Ok(())
}

/// 底层 diff：该 revision 中该文件相对其父 revision 的改动（unified diff 文本）。
/// 首条 revision（无父）返回提示文本。
pub fn file_diff(project_root: &Path, revision: &str, path: &str) -> Result<String> {
    let Some(parent) = revision_parent(project_root, revision)? else {
        return Ok("首次记录（无父版本，无改动差异）".to_string());
    };
    Ok(run_lore(
        project_root,
        &["file", "diff", path, "--source", &parent, "--target", revision],
    )?)
}

pub fn status_lines(project_root: &Path) -> Result<Vec<LoreStatusLine>> {
    let out = run_lore(project_root, &["status", "--scan"])?;
    Ok(parse_status(&out))
}

/// 当前分支 latest revision 签名（status 头行 "On branch main revision N -> <sig>"）
pub fn current_revision(project_root: &Path) -> Result<String> {
    let out = run_lore(project_root, &["status", "--revision-only"])?;
    out.lines()
        .find_map(|l| {
            let t = l.trim();
            if t.starts_with("On branch") {
                t.split("-> ").nth(1).map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .context("status --revision-only 输出解析失败：未找到 On branch 行")
}

/// 回退/撤销回退/顶替：重置分支 latest 指针并同步工作区到指定 revision。
/// 要求无 staged state、无 dirty 文件（否则 lore 拒绝）。
pub fn branch_reset(project_root: &Path, revision: &str) -> Result<()> {
    run_lore(project_root, &["branch", "reset", revision])?;
    Ok(())
}

/// 撤销某 revision 的改动（产生新 revision，冲突时进入 staged 冲突状态）
pub fn revert(project_root: &Path, revision: &str) -> Result<()> {
    run_lore(project_root, &["revision", "revert", revision])?;
    Ok(())
}

pub fn parse_status(out: &str) -> Vec<LoreStatusLine> {
    let mut lines = Vec::new();
    let mut section = "";
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match t {
            "Untracked files:" => { section = "untracked"; continue }
            "Changes not staged for commit:" => { section = "notstaged"; continue }
            "Changes staged for commit:" => { section = "staged"; continue }
            "Changes in conflict:" => { section = "conflict"; continue }
            _ => {}
        }
        let Some(first) = t.chars().next() else { continue };
        if !matches!(first, 'A' | 'M' | 'D') {
            continue;
        }
        let Some(rest) = t.get(1..) else { continue };
        if !rest.starts_with(' ') {
            continue;
        }
        let mut path = rest.trim().to_string();
        path = path.trim_end_matches('!').trim_end().to_string();
        path = path.trim_end_matches("(M)").trim_end().to_string();
        if path.is_empty() {
            continue;
        }
        let state = match (first, section) {
            ('A', "untracked") => LoreState::Untracked,
            ('A', _) => LoreState::Added,
            ('M', _) => LoreState::Modified,
            ('D', _) => LoreState::Deleted,
            _ => LoreState::Unchanged,
        };
        lines.push(LoreStatusLine { path, state });
    }
    lines
}

pub fn server_available() -> bool {
    TcpStream::connect("127.0.0.1:41339").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_lore_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn require_server() {
        if !server_available() {
            panic!("loreserver 未运行：请先启动 `C:\\Users\\para\\AppData\\Local\\lore\\bin\\loreserver.exe`（无配置 demo 模式，端口 41337/41339）");
        }
    }

    #[test]
    fn test_create_repo_and_detect() {
        require_server();
        let root = tmp_root("create");
        assert!(!repo_exists(&root));
        create_repo(&root).unwrap();
        assert!(repo_exists(&root));
    }

    #[test]
    fn test_commit_and_history_with_metadata() {
        require_server();
        let root = tmp_root("commit");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "hello").unwrap();
        commit(&root, "first", &[("kind", "user")]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("user"));
    }

    #[test]
    fn test_status_lines_detect_untracked() {
        require_server();
        let root = tmp_root("status");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/b.txt"), "2").unwrap();
        let lines = status_lines(&root).unwrap();
        assert!(lines.iter().any(|l| l.path == "a.txt"));
        assert!(lines.iter().any(|l| l.path == "web/b.txt"));
    }

    #[test]
    fn test_commit_web_stages_only_web_and_skips_no_change() {
        require_server();
        let root = tmp_root("webcommit");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "user").unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/index.html"), "<html></html>").unwrap();
        commit_web(&root).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("web"));
        assert_eq!(hist[0].message, "web update");
        // 用户文件不应被 web 提交卷入（仍 untracked）
        let lines = status_lines(&root).unwrap();
        assert!(lines.iter().any(|l| l.path == "a.txt"), "a.txt 应保持未跟踪");
        assert!(!lines.iter().any(|l| l.path.starts_with("web/")), "web 文件应已提交");
        // 无改动时再次 commit_web 应跳过（不产生空提交）
        commit_web(&root).unwrap();
        assert_eq!(revision_history(&root).unwrap().len(), 1);
    }

    #[test]
    fn test_commit_web_skips_without_web_dir_or_empty() {
        require_server();
        let root = tmp_root("webmissing");
        create_repo(&root).unwrap();
        commit_web(&root).unwrap();
        assert_eq!(revision_history(&root).unwrap().len(), 0);
        std::fs::create_dir_all(root.join("web")).unwrap();
        commit_web(&root).unwrap();
        assert_eq!(revision_history(&root).unwrap().len(), 0);
    }

    #[test]
    fn test_commit_web_commits_timeline_json() {
        require_server();
        let root = tmp_root("webtl");
        create_repo(&root).unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/timeline.json"), "{}").unwrap();
        commit_web(&root).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("web"));
        // 再次改动 timeline.json 后重提（验证第二次 web 提交成功）
        std::fs::write(root.join("web/timeline.json"), "{\"x\":1}").unwrap();
        commit_web(&root).unwrap();
        assert_eq!(crate::lore_project::web_revision_count(&root).unwrap(), 2);
    }

    #[test]
    fn test_export_files() {
        require_server();
        let root = tmp_root("export");
        create_repo(&root).unwrap();
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(root.join("a.txt"), "hello v1").unwrap();
        std::fs::write(root.join("models/m.blend"), "blend1").unwrap();
        commit(&root, "first", &[("kind", "user")]).unwrap();
        let sig = revision_history(&root).unwrap()[0].id.clone();
        let dest = tmp_root("export_dest");
        export_files(&root, &sig, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "hello v1");
        assert_eq!(std::fs::read_to_string(dest.join("models/m.blend")).unwrap(), "blend1");
    }

    #[test]
    fn test_file_diff() {
        require_server();
        let root = tmp_root("fdiff");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        commit(&root, "first", &[("kind", "user")]).unwrap();
        std::fs::write(root.join("a.txt"), "v2 changed").unwrap();
        commit(&root, "second", &[("kind", "user")]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 2);
        assert!(!hist[0].date.is_empty());
        let diff = file_diff(&root, &hist[0].id, "a.txt").unwrap();
        assert!(diff.contains("---"));
        assert!(diff.contains("+++"));
        let first = file_diff(&root, &hist[1].id, "a.txt").unwrap();
        assert!(first.is_empty() || first.contains("首次记录"));
    }
}
