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
    if paths.is_empty() {
        return commit(project_root, message, metadata);
    }
    let mut args: Vec<&str> = vec!["stage"];
    for p in paths {
        args.push(p);
    }
    run_lore(project_root, &args)?;
    commit_staged(project_root, message, metadata)
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

fn find_signature(project_root: &Path, number: u32) -> Result<String> {
    let out = run_lore(project_root, &["revision", "find", "number", &number.to_string()])?;
    Ok(out.trim().to_string())
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

pub fn revision_history(project_root: &Path) -> Result<Vec<LoreRevision>> {
    let out = run_lore(project_root, &["history", "--oneline"])?;
    let mut revs = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((num, msg)) = line.split_once(' ') else { continue };
        let Ok(number) = num.parse::<u32>() else { continue };
        let id = find_signature(project_root, number)?;
        let metadata = revision_metadata(project_root, &id).unwrap_or_default();
        revs.push(LoreRevision { number, id, message: msg.trim().to_string(), metadata });
    }
    Ok(revs)
}

pub fn status_lines(project_root: &Path) -> Result<Vec<LoreStatusLine>> {
    let out = run_lore(project_root, &["status", "--scan"])?;
    Ok(parse_status(&out))
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
}
