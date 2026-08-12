use std::collections::HashMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{bail, Context, Result};

/// Lore 最低/锁定版本（前置依赖；build.rs 捆绑与此一致）
pub const LORE_MIN_VERSION: &str = "0.8.6";
/// 平台默认 lore 路径（无 env/捆绑时兜底；Linux 走 PATH）
#[cfg(target_os = "windows")]
pub const LORE_BIN: &str = "C:\\Users\\para\\AppData\\Local\\lore\\bin\\lore.exe";
#[cfg(not(target_os = "windows"))]
pub const LORE_BIN: &str = "lore";
pub const LORE_SERVER_URL: &str = "lore://127.0.0.1:41337";

/// 运行时捆绑目录（GUI 安装后由 src-tauri 注入 resource_dir/lore-bin；CLI 走 exe 同目录）
static BUNDLED_LORE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_bundled_lore_dir(dir: PathBuf) {
    let _ = BUNDLED_LORE_DIR.set(dir);
}

/// 捆绑目录查找：显式注入（Tauri 安装包）→ exe 同目录 lore-bin（分发两件套）→ 编译期注入（dev 构建路径）
pub fn bundled_lore_dir() -> Option<PathBuf> {
    if let Some(d) = BUNDLED_LORE_DIR.get() {
        if d.is_dir() {
            return Some(d.clone());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("lore-bin");
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let injected = env!("KEINESTAMP_BUNDLED_LORE_DIR");
    let p = PathBuf::from(injected);
    if p.is_dir() {
        return Some(p);
    }
    None
}

pub fn lore_bin() -> String {
    if let Ok(p) = std::env::var("KEINESTAMP_LORE_BIN") {
        return p;
    }
    if let Some(dir) = bundled_lore_dir() {
        let name = if cfg!(target_os = "windows") { "lore.exe" } else { "lore" };
        let p = dir.join(name);
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }
    LORE_BIN.to_string()
}

/// 从 `--version` 输出解析主版本：`lore 0.8.6+373` / `loreserver 0.8.6` → (0, 8, 6)
pub fn parse_lore_version(out: &str) -> Option<(u32, u32, u32)> {
    let bytes = out.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut segs = [0u32; 3];
            let mut n = 0usize;
            let mut j = i;
            while j < bytes.len() && n < 3 {
                let start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if start == j {
                    break;
                }
                segs[n] = out[start..j].parse::<u32>().ok()?;
                n += 1;
                if j < bytes.len() && bytes[j] == b'.' {
                    j += 1;
                } else {
                    break;
                }
            }
            if n == 3 {
                return Some((segs[0], segs[1], segs[2]));
            }
        }
        i += 1;
    }
    None
}

#[derive(Debug, Clone)]
pub struct LoreEnvInfo {
    pub bin: String,
    pub version: String,
}

/// 前置依赖检测：lore 可执行 + 版本精确匹配 LORE_MIN_VERSION（0.8.6）。
/// 错误区分「未找到 / 版本不符 / 解析失败」。
pub fn check_lore_env() -> Result<LoreEnvInfo> {
    check_lore_env_at(&lore_bin())
}

/// 指定 bin 检测（可测试；不依赖全局环境）
pub fn check_lore_env_at(bin: &str) -> Result<LoreEnvInfo> {
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .map_err(|e| anyhow::anyhow!("未找到 lore（{}）：{}（请重新构建，build.rs 自动捆绑 v{}）", bin, e, LORE_MIN_VERSION))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let Some(v) = parse_lore_version(&stdout) else {
        bail!("lore 版本解析失败：{}（{}）", stdout.trim(), bin);
    };
    let want = parse_lore_version(LORE_MIN_VERSION)
        .expect("LORE_MIN_VERSION 解析失败");
    if v != want {
        bail!(
            "需要 Lore v{}，检测到 {}.{}.{}（{}）——请重新构建（build.rs 自动捆绑 v{}）",
            LORE_MIN_VERSION, v.0, v.1, v.2, bin, LORE_MIN_VERSION
        );
    }
    Ok(LoreEnvInfo { bin: bin.to_string(), version: format!("{}.{}.{}", v.0, v.1, v.2) })
}

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

/// lore 调用全局串行锁：同一时刻只有一个 lore 进程（多进程并发打开同一 store 会锁竞争/挂起）
static LORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// lore 命令执行超时（挂起兜底，60s）
const LORE_CMD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// 在仓库目录内执行 lore（**本地离线模式**：强制 --offline 跳过 remote 连接——在线仓库 + 服务器离线时
/// 每个命令会等 2-4s 连接超时；串行 + 超时兜底）
pub fn run_lore(project_root: &Path, args: &[&str]) -> Result<String> {
    run_lore_at(Some(project_root), args, true)
}

/// 远程命令（push/sync 等必须连服务器）：不带 --offline
pub fn run_lore_remote(project_root: &Path, args: &[&str]) -> Result<String> {
    run_lore_at(Some(project_root), args, false)
}

/// 执行 lore（cwd 可选；串行 + 超时兜底）。所有 lore spawn 的唯一出口。
fn run_lore_at(cwd: Option<&Path>, args: &[&str], offline: bool) -> Result<String> {
    let _guard = LORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut cmd = Command::new(lore_bin());
    let mut args_owned: Vec<&str> = args.to_vec();
    if offline {
        args_owned.push("--offline");
    }
    // 阻止自动增量 GC：在线仓库（push 过，远端有副本）的 GC 会清理本地历史块，
    // 导致回退/读取旧 revision 报 Address not found（36_mine2 事故根因）；只读命令无副作用
    args_owned.push("--no-gc");
    cmd.args(&args_owned)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().context("failed to spawn lore")?;
    let deadline = std::time::Instant::now() + LORE_CMD_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    bail!("lore {:?} 超时（{}s），已终止", args, LORE_CMD_TIMEOUT.as_secs());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => bail!("lore wait failed: {}", e),
        }
    }
    let out = child.wait_with_output().context("failed to read lore output")?;
    if !out.status.success() {
        bail!("lore {:?} failed: {}", args, translate_lore_error(&String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// lore 底层错误转译（保留原文）：Address not found = 该 revision 的数据已不在本地存储。
pub fn translate_lore_error(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.contains("Address not found") {
        format!(
            "{}（该记录的数据已不在本地存储：可能被存储清理或尚未从服务器下载，无法读取）",
            trimmed
        )
    } else {
        trimmed.to_string()
    }
}

pub fn repo_exists(project_root: &Path) -> bool {
    project_root.join(".lore").is_dir()
}

/// init 提交：`.loreignore` only（metadata `kind=init`，GUI 时间线过滤不显示）。
/// 让首条用户记录有父——回退首条用户记录 = 回到 init 提交（工作区只剩 .loreignore，
/// 用户文件变 untracked，changes 区可重新勾选/修改），而非"reset 自身无差异"。
/// 清理全部残留 staged state（`lore unstage`）：status 解析 "Changes staged for commit" 段的
/// 文件（A/M/D）逐个 unstage。防止异常残留（回退失败中间态等）被下次 commit 一并提交
/// （lore commit = 提交全部 staged）。未 staged 路径 unstage 也安全（"No changes unstaged" exit 0）。
pub fn unstage_all(project_root: &Path) -> Result<()> {
    let out = run_lore(project_root, &["status"])?;
    let mut staged: Vec<String> = Vec::new();
    let mut in_staged = false;
    for line in out.lines() {
        let l = line.trim_start();
        if l == "Changes staged for commit:" {
            in_staged = true;
            continue;
        }
        if in_staged {
            if l.is_empty() {
                break;
            }
            // 注意：char 数组 Pattern 是序列匹配（"AMD"），非任一成员 → 用首字符判断；
            // 路径尾随空格（status 行格式 `A <path> `）必须 trim，否则 unstage 路径不匹配
            let first = l.chars().next();
            if matches!(first, Some('A') | Some('M') | Some('D')) {
                staged.push(l[1..].trim().to_string());
            }
        }
    }
    if staged.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["unstage"];
    for p in &staged {
        args.push(p);
    }
    run_lore(project_root, &args)?;
    Ok(())
}

/// 幂等：已有历史（补洞）或 .loreignore 缺失 → 跳过。
pub fn init_commit(project_root: &Path) -> Result<()> {
    let loreignore = project_root.join(crate::hidden::LOREIGNORE);
    if !loreignore.is_file() || !revision_history(project_root)?.is_empty() {
        return Ok(());
    }
    unstage_all(project_root)?;
    let out = run_lore(project_root, &["--force", "stage", crate::hidden::LOREIGNORE])?;
    if out.contains("[Error]") || out.contains("No changes staged") {
        return Ok(());
    }
    commit_staged(project_root, "init", &[("kind", "init")])?;
    Ok(())
}

/// 随机仓库名（32 hex，纯 ASCII，通过 Lore 名校验；与本地目录名解耦）
fn random_repo_name() -> String {
    let mut buf = [0u8; 16];
    let _ = getrandom::getrandom(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 纯本地建仓（`lore --offline repository create <随机hash名>`，不连服务器、不注册）。
/// 仓库名 = 随机 32 hex（Lore 拒绝非 ASCII 名，目录名可任意含中文）。
/// config.toml 的 remote_url 为空 → push 不可用（"No remote configured"），备份后续按需补。
pub fn create_repo(project_root: &Path) -> Result<String> {
    let name = random_repo_name();
    run_lore(project_root, &["--offline", "repository", "create", &name])?;
    Ok(name)
}

pub fn commit(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String> {
    // 先清残留 staged（防止异常残留被一并提交），再 stage --scan 全量变更
    unstage_all(project_root)?;
    run_lore(project_root, &["stage", "--scan", "."])?;
    commit_staged(project_root, message, metadata)
}

pub fn commit_files(project_root: &Path, message: &str, metadata: &[(&str, &str)], paths: &[String]) -> Result<String> {
    // 先清残留 staged（防止异常残留被一并提交），再按勾选 stage
    unstage_all(project_root)?;
    // 目录路径不参与 stage（`lore stage <目录>` = 目录下全部 dirty 文件 → 未勾选文件被提交）
    let file_paths: Vec<&str> = paths
        .iter()
        .filter(|p| !p.ends_with('/'))
        .map(String::as_str)
        .collect();
    if !file_paths.is_empty() {
        let mut args: Vec<&str> = vec!["stage"];
        args.extend(file_paths);
        run_lore(project_root, &args)?;
    }
    // .loreignore 自动记录：UI 隐藏它，但每次提交强制携带其变更进历史（点开头 --force）
    let _ = run_lore(project_root, &["--force", "stage", crate::hidden::LOREIGNORE]);
    commit_staged(project_root, message, metadata)
}

fn commit_staged(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String> {
    if !metadata.is_empty() {
        let _ = run_lore(project_root, &["revision", "metadata", "clear"]);
        for (k, v) in metadata {
            run_lore(project_root, &["revision", "metadata", "set", k, v])?;
        }
    }
    // 提交身份：程序级全局（AppConfig，默认 keinestamp@local）——commit 带 --identity 参数，
    // 不依赖服务器解析身份（在线仓库 + 服务器离线也能记录）
    let identity = crate::app_config::get_lore_identity();
    let out = run_lore(project_root, &["--identity", &identity, "commit", message])?;
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
                // lore 配置身份后会在 message 块附加 Creator/Committer 行 → 过滤（保留真实 message）
                if !trimmed.is_empty()
                    && !trimmed.starts_with("Creator")
                    && !trimmed.starts_with("Committer")
                {
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
    // 优先读 timeline 缓存（主线 entries 带 parent，零 lore 调用——在线仓库 + 服务器离线时
    // `revision info` 会等 2-4s 连接超时）；缓存缺 parent（旧缓存/被顶替记录）→ fallback lore
    if let Ok(tl) = crate::timeline::read_timeline(&crate::timeline::timeline_cache_path(project_root)) {
        if let Some(e) = tl.entries.iter().find(|e| e.id == revision) {
            if !e.parent.is_empty() {
                return Ok(Some(e.parent.clone()));
            }
        }
    }
    let out = run_lore(project_root, &["revision", "info", revision])?;
    Ok(out.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("Parent")
            .map(|s| s.trim_start_matches([' ', ':']).trim().to_string())
    }))
}

/// revision 相对其父的改动列表（(A/M/D, 路径)），供回退"检出 X 改动"应用。
/// 过滤程序产物：`.keinestamp/`（整体隔离后新项目不入库，仅旧项目历史防御——
/// 检出它们会因点开头/loreignore 校验失败）+ `.loreignore`（自动记录文件，检出无意义）。
pub fn revision_changes(project_root: &Path, revision: &str) -> Result<Vec<(char, String)>> {
    let Some(parent) = revision_parent(project_root, revision)? else {
        return Ok(Vec::new()); // 首条记录无父：无改动可检出
    };
    let out = run_lore(project_root, &["revision", "diff", &parent, "--target", revision])?;
    let private_root = crate::hidden::PRIVATE_DIR.trim_end_matches('/');
    let changes: Vec<(char, String)> = parse_revision_changes(&out)
        .into_iter()
        .filter(|(_, p)| {
            !p.starts_with(crate::hidden::PRIVATE_DIR)
                && p != private_root
                && p != crate::hidden::LOREIGNORE
        })
        .collect();
    // diff 的目录行无尾随 /（`A refer`，与 status 的 `A refer/` 不同）——前缀匹配过滤：
    // 某路径是其他路径的前缀（refer ⊂ refer/back.png）→ 目录行，丢弃
    Ok(changes
        .iter()
        .filter(|(_, p)| {
            !changes
                .iter()
                .any(|(_, q)| q != p && q.starts_with(&format!("{}/", p)))
        })
        .cloned()
        .collect())
}

pub fn parse_revision_changes(out: &str) -> Vec<(char, String)> {
    out.lines()
        .filter_map(|l| {
            let t = l.trim();
            let mut chars = t.chars();
            let status = chars.next()?;
            if !matches!(status, 'A' | 'M' | 'D') {
                return None;
            }
            let rest = t[status.len_utf8()..].trim();
            if rest.is_empty() {
                None
            } else {
                Some((status, rest.to_string()))
            }
        })
        .collect()
}

/// 强制补 init 提交（旧项目回退首条记录时用）：不管历史，stage `.loreignore` 并提交
/// （metadata `kind=init`，GUI 过滤不显示）→ HEAD 树变为仅 .loreignore（工作区用户文件
/// 变 untracked，changes 区可重新勾选/修改）。返回新 init 提交完整签名。
pub fn force_init_commit(project_root: &Path) -> Result<String> {
    // 注意：不能 unstage_all——调用方（首条回退 None 分支）已 stage 删除的树内文件，
    // 这里只追加 .loreignore 后提交（staged = D 删除 + .loreignore → 树 = 仅 .loreignore）
    let out = run_lore(project_root, &["--force", "stage", crate::hidden::LOREIGNORE])?;
    if out.contains("[Error]") {
        bail!("init 提交 stage 失败：{}", out);
    }
    commit_staged(project_root, "init", &[("kind", "init")])
}

/// 目录级导出：`repository dump` 解析文件树 → 逐文件 `file write` 到 dest（v1 export_entry 对应）。
/// dump 文件行含 ` addr `，目录行以 `/` 结尾（含 ` child N`），仅解析前者。
/// 路径可能含空格（`x 拷贝.png`）→ 用 `" id "` 分隔取完整路径（split_whitespace 会截断）。
pub fn export_files(project_root: &Path, revision: &str, dest: &Path) -> Result<()> {
    let out = run_lore(project_root, &["repository", "dump", "--revision", revision])?;
    let mut written = 0usize;
    for line in out.lines() {
        if !line.contains(" addr ") {
            continue;
        }
        let Some(sep) = line.find(" id ") else { continue };
        let path = &line[..sep];
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

/// 探测 loreserver 健康（HTTP /health_check，端口 41339）。host 来自配置地址（默认 127.0.0.1）。
/// host 畸形（如误填 http://...）→ 解析失败返回 false（不 panic）。
pub fn lore_server_health(host: &str) -> bool {
    use std::io::{Read, Write};
    use std::time::Duration;
    let addr = format!("{}:41339", host);
    let Ok(addr) = addr.parse::<std::net::SocketAddr>() else {
        return false;
    };
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(800)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    let req = format!("GET /health_check HTTP/1.1\r\nHost: {}:41339\r\nConnection: close\r\n\r\n", host);
    let _ = s.write_all(req.as_bytes());
    let mut buf = [0u8; 128];
    let n = s.read(&mut buf).unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
}

/// 仓库是否在线创建（config.toml remote_url 非空——离线仓库 push 数据不到服务器，备份不支持）
pub fn has_remote_url(project_root: &Path) -> bool {
    let cfg = project_root.join(".lore").join("config.toml");
    match std::fs::read_to_string(cfg) {
        Ok(c) => c
            .lines()
            .find_map(|l| l.trim().strip_prefix("remote_url"))
            .map(|v| {
                // ` = ""` → trim → `= ""` → 删前导 = → ` ""` → trim → `""` → 删引号 → 空
                let v = v.trim().trim_start_matches('=').trim().trim_matches('"').trim();
                !v.is_empty()
            })
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// 推送全部本地 revision 到远端（备份链路，实测 §9 + 2026-08-11 备份实测）：
/// 1. 仓库须为**在线创建**（remote_url 非空；离线仓库 push 假成功，数据不到服务器）→ 明确报错
/// 2. 读配置地址（app-config lore_server_url，默认 lore://127.0.0.1:41337）→ 探测 health（http://host:41339/health_check）→ 不可达报错
/// 3. `lore push`（首推自动建分支，增量推送）
pub fn push_to_server(project_root: &Path) -> Result<String> {
    if !has_remote_url(project_root) {
        bail!("该项目为离线创建，暂不支持备份（备份需在线创建仓库）");
    }
    let url = crate::app_config::get_lore_server_url();
    let host = url
        .trim_start_matches("lore://")
        .split(':')
        .next()
        .unwrap_or("127.0.0.1");
    if !lore_server_health(host) {
        bail!("loreserver 不可达（{}），请确认服务端已启动", url);
    }
    run_lore_remote(project_root, &["push"])
}

/// 列出服务器上的仓库（`lore repository list <url>`，实测输出每行 `name (id)`）。
/// 需 loreserver 在线，否则返回错误。
pub fn list_server_repos(url: &str) -> Result<Vec<(String, String)>> {
    let host = url
        .trim_start_matches("lore://")
        .split(':')
        .next()
        .unwrap_or("127.0.0.1");
    if !lore_server_health(host) {
        bail!("loreserver 不可达（{}），请确认服务端已启动", url);
    }
    let out = run_lore_global(&["repository", "list", url])?;
    let mut repos = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        let Some(open) = t.find('(') else { continue };
        let Some(close) = t.rfind(')') else { continue };
        let name = t[..open].trim().to_string();
        let id = t[open + 1..close].trim().to_string();
        if !name.is_empty() && !id.is_empty() {
            repos.push((name, id));
        }
    }
    Ok(repos)
}

/// 从服务器克隆仓库到本地（`lore clone <url>/<name> <dest>`，实测 §9）。
/// dest_parent 下若已存在同名目录（非空/已初始化）→ 报错（重名检测，不覆盖）。
/// 返回克隆后的仓库目录。
pub fn clone_repo_to(url: &str, name: &str, dest_parent: &Path) -> Result<PathBuf> {
    let dest = dest_parent.join(name);
    if dest.exists() {
        bail!("本地已存在同名目录：{}", dest.display());
    }
    std::fs::create_dir_all(&dest)?;
    let clone_url = format!("{}/{}", url.trim_end_matches('/'), name);
    let out = match run_lore_global(&["clone", &clone_url, dest.to_string_lossy().as_ref()]) {
        Ok(o) => o,
        Err(e) => {
            // 失败清理预建目录（否则下次 clone 撞"已存在同名目录"）
            let _ = std::fs::remove_dir_all(&dest);
            return Err(e);
        }
    };
    if out.contains("[Error]") {
        let _ = std::fs::remove_dir_all(&dest);
        bail!("克隆失败：{}", out.trim());
    }
    // clone 只下载最新状态，历史块按需从远端拉取；拉全量历史块，
    // 否则 --offline 的 history/回退/记录会因本地缺块报 Address not found（尽力而为）
    let _ = fetch_full_history(&dest);
    Ok(dest)
}

/// clone 后拉取全量历史块（逐 revision `repository dump`，在线拉块落到本地 store）。
/// 尽力而为：某 revision 数据已不可得时跳过，不阻止 clone 成功。
pub fn fetch_full_history(project_root: &Path) -> Result<()> {
    let out = run_lore_remote(project_root, &["history"])?;
    let sigs: Vec<String> = out
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix("Signature")
                .map(|s| s.trim_start_matches([' ', ':']).trim().to_string())
        })
        .collect();
    for s in sigs {
        let _ = run_lore_remote(project_root, &["repository", "dump", "--revision", &s]);
    }
    Ok(())
}

/// 删除服务器上的仓库（`lore repository delete <url>/<name>`，实测：`Repository deleted successfully`；
/// 不存在报 `[Error] Not found`）。删除 = 备份数据丢失，调用方需确认。
pub fn delete_server_repo(url: &str, name: &str) -> Result<()> {
    let host = url
        .trim_start_matches("lore://")
        .split(':')
        .next()
        .unwrap_or("127.0.0.1");
    if !lore_server_health(host) {
        bail!("loreserver 不可达（{}），请确认服务端已启动", url);
    }
    let delete_url = format!("{}/{}", url.trim_end_matches('/'), name);
    run_lore_global(&["repository", "delete", &delete_url])?;
    Ok(())
}

/// 离线仓库迁移为在线（2026-08-11 实测：`repository create <url>/<名> --id <同ID>` 预注册，
/// 服务器按 ID 识别 → push 真正生效，本地 store/hash/历史全保留）。
/// 步骤：校验离线+无回退态 → 读 .lore/id（16 字节 → 32hex）→ 临时目录 create --id 注册 →
/// 设置 config.toml remote_url → `lore push` 全量。push 失败时清理服务器残留注册。
pub fn migrate_offline_to_online(project_root: &Path) -> Result<()> {
    if has_remote_url(project_root) {
        bail!("该仓库已是在线仓库，无需迁移");
    }
    if crate::timeline::read_marker(project_root)?.is_some() {
        bail!("存在回退状态，请先恢复或记录后再迁移");
    }
    let id_bytes = std::fs::read(project_root.join(".lore").join("id"))
        .context("读取仓库 ID 失败（.lore/id）")?;
    let id: String = id_bytes.iter().map(|b| format!("{:02x}", b)).collect();

    let url = crate::app_config::get_lore_server_url();
    let host = url
        .trim_start_matches("lore://")
        .split(':')
        .next()
        .unwrap_or("127.0.0.1");
    if !lore_server_health(host) {
        bail!("loreserver 不可达（{}），请确认服务端已启动", url);
    }
    let name = ascii_repo_name(
        &project_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    // 临时目录 create --id 预注册（服务器按 ID 绑定名字）
    let tmp = std::env::temp_dir().join(format!("keinestamp_mig_{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let create_url = format!("{}/{}", url.trim_end_matches('/'), name);
    let out = match run_lore_in(&tmp, &["repository", "create", &create_url, "--id", &id]) {
        Ok(o) => o,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    let _ = std::fs::remove_dir_all(&tmp);
    if out.contains("[Error]") {
        bail!("服务器注册失败：{}", out.trim());
    }
    // 设置 remote_url（config.toml，幂等）
    let cfg_path = project_root.join(".lore").join("config.toml");
    let cfg = std::fs::read_to_string(&cfg_path)?;
    let cfg = cfg.replace("remote_url = \"\"", &format!("remote_url = \"{}\"", url));
    std::fs::write(&cfg_path, cfg)?;
    // 全量 push（远程命令：不带 --offline）
    if let Err(e) = run_lore_remote(project_root, &["push"]) {
        // 清理服务器残留注册（空仓库），本地不受影响
        let _ = delete_server_repo(&url, &name);
        return Err(e);
    }
    Ok(())
}

/// 目录名 → lore 合法仓库名（ASCII 字母数字 + `/_-.`，其余 → `_`）
pub fn ascii_repo_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "repo".to_string()
    } else {
        s
    }
}

/// 在指定 cwd 运行 lore（create 等需要 cwd 的命令；**远程命令**：不带 --offline）
fn run_lore_in(cwd: &Path, args: &[&str]) -> Result<String> {
    run_lore_at(Some(cwd), args, false)
}

/// 全局（非仓库内）lore 命令：clone/list/delete 等**远程命令**（不带 --offline）
fn run_lore_global(args: &[&str]) -> Result<String> {
    run_lore_at(None, args, false)
}

/// 从远端同步到本地（要求无 staged state；远程命令：不带 --offline）。
pub fn sync_from_server(project_root: &Path) -> Result<String> {
    run_lore_remote(project_root, &["sync"])
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
        // 仅 push 相关测试需要服务器（离线仓库 push 会 "No remote configured"）
        if !server_available() {
            panic!("loreserver 未运行：请先启动 loreserver.exe（局域网备份测试前置）");
        }
    }

    #[test]
    #[ignore = "需 loreserver 在线"]
    fn test_migrate_offline_to_online_end_to_end() {
        require_server();
        let root = tmp_root("migrate");
        create_repo(&root).unwrap();
        // 大文件多版本：clone 只下载最新状态，历史块需 fetch_full_history 拉全
        let big1 = vec![0x41u8; 1572864];
        std::fs::write(root.join("a.bin"), &big1).unwrap();
        crate::lore_project::record(&root, "first", None, None, &vec![".keinestamp/".to_string()]).unwrap();
        let big2 = vec![0x42u8; 1572864];
        std::fs::write(root.join("a.bin"), &big2).unwrap();
        crate::lore_project::record(&root, "second", Some("建模"), None, &vec![".keinestamp/".to_string()]).unwrap();
        assert!(!has_remote_url(&root), "前置：离线仓库");

        migrate_offline_to_online(&root).unwrap();
        assert!(has_remote_url(&root), "迁移后 remote_url 非空");
        // 服务器上可 clone（名字 = 目录名 ascii 化）
        let cl_parent = tmp_root("migrate_cl");
        let name = ascii_repo_name(&root.file_name().unwrap().to_string_lossy());
        let url = crate::app_config::get_lore_server_url();
        let cloned = clone_repo_to(&url, &name, &cl_parent).unwrap();
        assert_eq!(
            std::fs::read(cloned.join("a.bin")).unwrap(),
            big2,
            "clone 内容 = 最新版本"
        );
        // --offline 读全量历史（依赖 fetch_full_history 拉全历史块，否则 Address not found）
        let hist = revision_history(&cloned).unwrap();
        assert_eq!(hist.len(), 2, "历史完整重放（2 条）");
        assert_eq!(hist[0].message, "second");
        assert_eq!(hist[0].metadata.get("tag").map(String::as_str), Some("建模"));
        let _ = std::fs::remove_dir_all(&cl_parent);
        // 清理服务器仓库
        delete_server_repo(&url, &name).unwrap();
    }

    #[test]
    fn test_translate_lore_error() {
        let s = translate_lore_error("[Error] Address not found: abc-00000000000000000000000000000000");
        assert!(s.contains("Address not found"), "保留原文: {s}");
        assert!(s.contains("已不在本地存储"), "转译说明: {s}");
        let other = translate_lore_error("[Error] File has local changes: a.txt");
        assert_eq!(other, "[Error] File has local changes: a.txt");
        assert_eq!(translate_lore_error(""), "");
    }

    #[test]
    fn test_parse_lore_version() {
        assert_eq!(parse_lore_version("lore 0.8.6+373"), Some((0, 8, 6)));
        assert_eq!(parse_lore_version("loreserver 0.8.6"), Some((0, 8, 6)));
        assert_eq!(parse_lore_version("lore 0.8.6"), Some((0, 8, 6)));
        assert_eq!(parse_lore_version("0.8.6+373"), Some((0, 8, 6)));
        assert_eq!(parse_lore_version("garbage no version"), None);
        assert_eq!(parse_lore_version(""), None);
        assert_eq!(parse_lore_version("1.2"), None);
    }

    #[test]
    fn test_check_lore_env_version_mismatch() {
        // 假 bin 路径（不碰全局 env，避免并行污染）→ 未找到分支
        let err = check_lore_env_at("C:\\nonexistent\\lore.exe").unwrap_err();
        assert!(err.to_string().contains("未找到 lore"), "err: {err}");
        // 指向真实命令但版本不符（cmd.exe 输出 Windows 版本 10.x）→ 版本不符分支
        let err2 = check_lore_env_at("cmd.exe").unwrap_err();
        assert!(
            err2.to_string().contains("需要 Lore") || err2.to_string().contains("解析失败") || err2.to_string().contains("未找到"),
            "err2: {err2}"
        );
    }

    #[test]
    fn test_check_lore_env_ok_with_real_lore() {
        // 真实环境：本机已装 lore 0.8.6（或捆绑目录命中）
        if let Ok(info) = check_lore_env() {
            assert_eq!(info.version, LORE_MIN_VERSION);
        } else {
            eprintln!("[skip] 当前环境无 lore 0.8.6（CI/无捆绑环境）");
        }
    }

    #[test]
    fn test_ascii_repo_name() {
        assert_eq!(ascii_repo_name("36_mine2"), "36_mine2");
        assert_eq!(ascii_repo_name("离线测试项目"), "______");
        assert_eq!(ascii_repo_name("My-Project_v1.2"), "My-Project_v1.2");
        assert_eq!(ascii_repo_name(""), "repo");
    }

    #[test]
    fn test_create_repo_and_detect() {
        let root = tmp_root("create");
        assert!(!repo_exists(&root));
        create_repo(&root).unwrap();
        assert!(repo_exists(&root));
    }

    #[test]
    fn test_commit_and_history_with_metadata() {
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
        let root = tmp_root("status");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::create_dir_all(root.join(".keinestamp").join("web")).unwrap();
        std::fs::write(root.join(".keinestamp/web/b.txt"), "2").unwrap();
        let lines = status_lines(&root).unwrap();
        assert!(lines.iter().any(|l| l.path == "a.txt"));
        assert!(lines.iter().any(|l| l.path == ".keinestamp/web/b.txt"));
    }

    #[test]
    fn test_export_files() {
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
    fn test_clone_repo_to_existing_dir_rejected() {
        let parent = std::env::temp_dir().join(format!("ks_clone_rej_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(parent.join("existing-repo")).unwrap();
        // 重名检测在 clone 之前（不依赖服务器）
        let err = clone_repo_to("lore://127.0.0.1:41337", "existing-repo", &parent).unwrap_err();
        assert!(err.to_string().contains("已存在同名目录"), "err: {err}");
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn test_list_server_repos_parse() {
        // 解析逻辑（不依赖服务器）：抽取为纯函数验证
        let out = "offrepo6 (019fefe0d9957930ba811c48d55b3aed)\nscratch5 (019febbb6c4577219245578ded710ca9)\n";
        let mut repos = Vec::new();
        for line in out.lines() {
            let t = line.trim();
            let Some(open) = t.find('(') else { continue };
            let Some(close) = t.rfind(')') else { continue };
            let name = t[..open].trim().to_string();
            let id = t[open + 1..close].trim().to_string();
            if !name.is_empty() && !id.is_empty() {
                repos.push((name, id));
            }
        }
        assert_eq!(
            repos,
            vec![
                ("offrepo6".to_string(), "019fefe0d9957930ba811c48d55b3aed".to_string()),
                ("scratch5".to_string(), "019febbb6c4577219245578ded710ca9".to_string()),
            ]
        );
    }

    #[test]
    fn test_has_remote_url() {
        let root = tmp_root("hasremote");
        create_repo(&root).unwrap();
        // 离线创建：remote_url 为空 → false
        let cfg_dbg = std::fs::read_to_string(root.join(".lore").join("config.toml")).unwrap(); eprintln!("CONFIG: {:?}", cfg_dbg); eprintln!("HAS_REMOTE: {}", has_remote_url(&root)); assert!(!has_remote_url(&root));
        // 改 config.toml remote_url → true
        let cfg_path = root.join(".lore").join("config.toml");
        let cfg = std::fs::read_to_string(&cfg_path).unwrap().replace("remote_url = \"\"", "remote_url = \"lore://127.0.0.1:41337\"");
        std::fs::write(&cfg_path, cfg).unwrap();
        assert!(has_remote_url(&root));
    }

    #[test]
    fn test_push_to_server_offline_repo_rejected() {
        let root = tmp_root("pushoffline");
        create_repo(&root).unwrap();
        // 离线仓库：即使服务器在线也明确报错（不产生假成功）
        let err = push_to_server(&root).unwrap_err();
        assert!(err.to_string().contains("离线创建"), "err: {err}");
    }

    #[test]
    fn test_push_to_server_server_unreachable() {
        let root = tmp_root("pushdown");
        create_repo(&root).unwrap();
        let cfg_path = root.join(".lore").join("config.toml");
        let cfg = std::fs::read_to_string(&cfg_path).unwrap().replace("remote_url = \"\"", "remote_url = \"lore://127.0.0.1:41337\"");
        std::fs::write(&cfg_path, cfg).unwrap();
        // 配置一个不可达地址 → health 探测失败报错（不 push）
        let old = crate::app_config::get_lore_server_url();
        crate::app_config::set_lore_server_url("lore://192.0.2.1:41337");
        let err = push_to_server(&root).unwrap_err();
        crate::app_config::set_lore_server_url(&old);
        assert!(err.to_string().contains("不可达"), "err: {err}");
    }

    #[test]
    #[ignore = "需要 loreserver 在线（局域网备份功能后续实现）"]
    fn test_push_to_server() {
        require_server();
        let root = tmp_root("push");
        // push 需要服务器注册：在线创建（离线 create 的 remote_url 为空）
        let name = format!("push{}", std::process::id());
        run_lore(&root, &["repository", "create", &format!("{}/{}", LORE_SERVER_URL, name)]).unwrap();
        std::fs::write(root.join("a.txt"), "hello").unwrap();
        commit(&root, "first", &[("kind", "user")]).unwrap();
        let out = push_to_server(&root).unwrap();
        assert!(out.contains("Pushed"), "push 输出应含 Pushed: {}", out);
        sync_from_server(&root).unwrap();
    }
}
