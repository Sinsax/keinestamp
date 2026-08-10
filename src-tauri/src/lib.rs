use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;
use tauri::Manager;

fn map_err<T>(r: anyhow::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

fn read_meta(path: &Path) -> Result<keinestamp::config::MetaConfig, String> {
    let meta_path = keinestamp::config::meta_path(path);
    map_err(keinestamp::config::read_meta(&meta_path))
}

fn state_code(state: &keinestamp::lore_backend::LoreState) -> &'static str {
    match state {
        keinestamp::lore_backend::LoreState::Added => "A",
        keinestamp::lore_backend::LoreState::Modified => "M",
        keinestamp::lore_backend::LoreState::Deleted => "D",
        keinestamp::lore_backend::LoreState::Untracked => "??",
        keinestamp::lore_backend::LoreState::Unchanged => "=",
    }
}

fn format_lore_date(s: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(s)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| s.to_string())
}

#[derive(Serialize)]
pub struct StatusResult {
    pub clean: bool,
    pub output: String,
}

#[derive(Serialize)]
pub struct LogEntry {
    pub hash: String,
    pub date: String,
    pub message: String,
}

#[tauri::command]
fn detect(path: String) -> Result<String, String> {
    eprintln!("[debug] detect({})", path);
    let pt = map_err(keinestamp::detect::detect_project(Path::new(&path)))?;
    let result = match pt {
        keinestamp::detect::ProjectType::Fresh => "fresh".to_string(),
        keinestamp::detect::ProjectType::Existing => "existing".to_string(),
        keinestamp::detect::ProjectType::Initialized(_) => "initialized".to_string(),
    };
    eprintln!("[debug] detect → {}", result);
    Ok(result)
}

#[tauri::command]
fn init_project(path: String) -> Result<keinestamp::config::MetaConfig, String> {
    eprintln!("[debug] init_project({})", path);
    let p = PathBuf::from(&path);
    map_err(keinestamp::init::init_project(&p))?;
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    keinestamp::recent::RecentProjects::add_path(&p);
    eprintln!("[debug] init_project → ok, project_root={}", meta.project_root.display());
    map_err(keinestamp::config::read_meta(&meta_path))
}

#[derive(Clone, serde::Serialize)]
struct CommitProgress {
    phase: String,
    message: String,
}

#[tauri::command]
async fn record(app: tauri::AppHandle, path: String, message: String, tag: Option<String>, no_preview: Option<bool>, files: Option<Vec<String>>) -> Result<String, String> {
    eprintln!("[debug] record({}, message={:?}, tag={:?}, no_preview={}, files={:?})", path, message, tag, no_preview.unwrap_or(false), files);
    let app_clone = app.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let emit = |phase: &str, msg: &str| {
            eprintln!("[debug] record-phase: {} — {}", phase, msg);
            let _ = app_clone.emit("commit-progress", CommitProgress {
                phase: phase.to_string(),
                message: msg.to_string(),
            });
        };

        emit("git_commit", "记录中...");

        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        let skip_preview = no_preview.unwrap_or(false);

        // 本次记录涉及的文件（Blender 预览判定用；selected 或全量 changed）
        let changed_files: Vec<String> = match &files {
            Some(sel) => sel.clone(),
            None => map_err(keinestamp::lore_project::changed_files(&meta.project_root, &hidden))?
                .into_iter()
                .map(|f| f.path)
                .collect(),
        };
        let has_blend = changed_files.iter().any(|f| f.ends_with(".blend"));

        // 记录（回退状态下 = 顶替：Lore branch reset 后 HEAD 即回退点，新记录自然顶替）；
        // 成功后清回退标记
        let hash: String = match files {
            Some(f) => map_err(keinestamp::lore_project::record(&meta.project_root, &message, Some(&f), &hidden))?,
            None => map_err(keinestamp::lore_project::record(&meta.project_root, &message, None, &hidden))?,
        };
        map_err(keinestamp::lore_project::clear_rollback(&meta.project_root))?;
        let short_hash = hash[..7.min(hash.len())].to_string();

        let blender_path_buf = if !skip_preview && has_blend {
            emit("blender_prepare", "准备 Blender...");
            match keinestamp::blender_download::ensure_blender() {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("[debug] record: Blender auto-download failed: {}, trying PATH", e);
                    keinestamp::app_config::get_blender_path()
                }
            }
        } else {
            keinestamp::app_config::get_blender_path()
        };
        let blender_path: Option<&Path> = blender_path_buf.as_deref();

        let preview_path = if !skip_preview && has_blend {
            let glb_config = keinestamp::blender_render::RenderConfig {
                output_dir: meta.web_root.join("previews_glb"),
                ..Default::default()
            };
            let mut glb_names: Vec<String> = Vec::new();
            for bf in &changed_files {
                if bf.ends_with(".blend") {
                    let safe_name = bf.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                    let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                    let bpath = meta.project_root.join(bf);
                    emit("blender_export", &format!("导出 GLB: {}", bf));
                    if let Err(e) = keinestamp::blender_render::export_to_glb(&bpath, &glb_config.output_dir, &glb_name, blender_path) {
                        eprintln!("[debug] record: GLB export skipped: {e}");
                    } else {
                        glb_names.push(glb_name);
                    }
                }
            }
            let _ = keinestamp::blender_render::clean_old_glb_previews(
                &meta.web_root.join("previews_glb"),
                glb_config.max_previews,
            );
            glb_names
                .iter()
                .map(|n| format!("previews_glb/{}.glb", n))
                .find(|rel| meta.web_root.join(rel).exists())
        } else {
            None
        };

        emit("timeline", "更新时间线...");

        // 时间线缓存已由 record() 内部 rebuild_cache 重建；补写预览路径
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        if let Some(preview) = &preview_path {
            if let Ok(mut tl) = keinestamp::timeline::read_timeline(&timeline_path) {
                if let Some(e) = tl.entries.iter_mut().find(|e| e.id == hash) {
                    e.preview = preview.clone();
                }
                let _ = keinestamp::timeline::write_timeline(&timeline_path, &tl);
            }
        }

        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        emit("html", "生成页面...");
        let timeline = map_err(keinestamp::timeline::read_timeline(&timeline_path))?;
        map_err(keinestamp::html_gen::generate_index_html(
            &meta.web_root,
            &meta.project_root,
            &timeline,
            &project_name,
        ))?;
        // web 产物（index.html/timeline.json/previews_glb）并入主仓库（kind=web）
        let _ = keinestamp::lore_backend::commit_web(&meta.project_root);

        keinestamp::recent::RecentProjects::add_path(&p);
        eprintln!("[debug] record → {}", short_hash);
        emit("done", "完成");
        Ok::<String, String>(short_hash)
    });
    let result: Result<String, String> = handle.await.map_err(|e| e.to_string())?;
    eprintln!("[debug] record result: {:?}", result);
    result
}

#[tauri::command]
async fn push(path: String) -> Result<(), String> {
    eprintln!("[debug] push({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        eprintln!("[debug] push: project_root={}", meta.project_root.display());

        if has_remote_impl(&meta.project_root) {
            map_err(keinestamp::lore_backend::run_lore(&meta.project_root, &["push"]))?;
        } else {
            eprintln!("[debug] push: no lore remote, skipping");
        }

        eprintln!("[debug] push → ok");
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn deploy(path: String) -> Result<(), String> {
    eprintln!("[debug] deploy({})", path);
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_p = keinestamp::config::meta_path(&p);
        let mut meta = map_err(keinestamp::config::read_meta(&meta_p))?;

        if meta.project_id.is_none() {
            meta.project_id = Some(keinestamp::config::generate_project_id());
            map_err(keinestamp::config::write_meta(&meta_p, &meta))?;
        }

        let server = keinestamp::app_config::get_deploy_server_url()
            .filter(|s| !s.is_empty());

        if let Some(ref server) = server {
            eprintln!("[debug] deploy: server={}", server);
            let deploy_token = keinestamp::app_config::get_deploy_token();
            let project_name = keinestamp::config::resolve_site_name(&meta);
            let project_id = meta.project_id.as_deref().unwrap_or("");
            map_err(keinestamp::push::deploy_to_server(
                &meta.web_root, server, deploy_token.as_deref(), meta.access_token.as_deref(), &project_name, project_id,
            ))?;
            meta.site_name = Some(project_name);
            meta.site_locked = true;
            map_err(keinestamp::config::write_meta(&meta_p, &meta))?;
            eprintln!("[debug] deploy → ok");
            Ok(())
        } else {
            Err("未设置服务器地址".to_string())
        }
    })
    .await
    .map_err(|e| format!("deploy task failed: {}", e))?
}

#[tauri::command]
async fn test_connection(server_url: String, deploy_token: Option<String>) -> Result<bool, String> {
    eprintln!("[debug] test_connection(server_url={:?})", server_url);
    tokio::task::spawn_blocking(move || {
        if server_url.is_empty() {
            return Ok(false);
        }
        let health_url = format!("{}/health", server_url.trim_end_matches('/'));
        eprintln!("[debug] test_connection: GET {}", health_url);

        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(15))
            .build();
        let mut req = agent.get(&health_url);
        if let Some(ref token) = deploy_token {
            if !token.is_empty() {
                req = req.set("Authorization", &format!("Bearer {}", token));
            }
        }

        match req.call() {
            Ok(resp) => {
                let ok = resp.status() == 200;
                eprintln!("[debug] test_connection → {}", if ok { "connected" } else { "failed" });
                Ok(ok)
            }
            Err(e) => {
                eprintln!("[debug] test_connection → error: {}", e);
                Ok(false)
            }
        }
    })
    .await
    .map_err(|e| format!("test_connection task failed: {}", e))?
}

#[derive(Clone, serde::Serialize)]
struct DeployStatus {
    local: u64,
    remote: Option<u64>,
}

#[tauri::command]
async fn get_deploy_status(
    server_url: String,
    deploy_token: Option<String>,
    path: String,
) -> Result<DeployStatus, String> {
    eprintln!("[debug] get_deploy_status({})", path);
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        let local = map_err(keinestamp::lore_project::web_revision_count(&meta.project_root))?;
        if !meta.site_locked {
            eprintln!("[debug] get_deploy_status: not deployed (site_locked=false), skip request");
            return Ok(DeployStatus { local, remote: None });
        }
        let site = keinestamp::config::resolve_site_name(&meta);
        let url = format!("{}/{}/version", server_url.trim_end_matches('/'), site);
        eprintln!("[debug] get_deploy_status: local={}, GET {}", local, url);
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(15))
            .build();
        let mut req = agent.get(&url);
        if let Some(ref token) = deploy_token {
            if !token.is_empty() {
                req = req.set("Authorization", &format!("Bearer {}", token));
            }
        }
        let remote = req
            .call()
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("count").and_then(|c| c.as_u64()));
        eprintln!("[debug] get_deploy_status → remote={:?}", remote);
        Ok(DeployStatus { local, remote })
    })
    .await
    .map_err(|e| format!("get_deploy_status task failed: {}", e))?
}

#[tauri::command]
async fn pull(path: String) -> Result<(), String> {
    eprintln!("[debug] pull({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        eprintln!("[debug] pull: project_root={}", meta.project_root.display());

        if has_remote_impl(&meta.project_root) {
            map_err(keinestamp::lore_backend::run_lore(&meta.project_root, &["sync"]))?;
        } else {
            eprintln!("[debug] pull: no lore remote, skipping");
        }

        eprintln!("[debug] pull → ok");
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

/// lore 仓库 remote 判定：`.lore/config.toml` 含 remote_url 即视为有远端
/// （本机 loreserver 恒在线，config.toml 的 remote_url 总是存在 → 正常工程返回 true）
fn has_remote_impl(project_root: &Path) -> bool {
    let cfg = project_root.join(".lore").join("config.toml");
    std::fs::read_to_string(cfg)
        .map(|c| c.contains("remote_url"))
        .unwrap_or(false)
}

#[tauri::command]
fn has_remote(path: String) -> bool {
    eprintln!("[debug] has_remote({})", path);
    let p = PathBuf::from(&path);
    let result = match keinestamp::config::read_meta(&keinestamp::config::meta_path(&p)) {
        Ok(meta) => has_remote_impl(&meta.project_root),
        Err(_) => false,
    };
    eprintln!("[debug] has_remote → {}", result);
    result
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {}", path));
    }
    let canonical = std::fs::canonicalize(&p).unwrap_or(p);
    let s = canonical.to_string_lossy();
    let clean = if cfg!(target_os = "windows") && s.starts_with(r"\\?\") {
        &s[4..]
    } else {
        &s
    };
    let path_str = clean.to_string();
    #[cfg(target_os = "windows")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("explorer")).arg(&path_str).spawn();
    #[cfg(target_os = "macos")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("open")).arg(&path_str).spawn();
    #[cfg(target_os = "linux")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open")).arg(&path_str).spawn();
    result.map_err(|e| format!("failed to open folder: {}", e))?;
    Ok(())
}

#[tauri::command]
fn get_ignore_info(project_root: String, files: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    eprintln!("[debug] get_ignore_info({})", project_root);
    let _ = files;
    let ignored = keinestamp::hidden::load_hidden(Path::new(&project_root));
    // v2：tracked 概念不存在（Lore 无 git index），隐藏列表即 ignored；
    // rule_ignored 由 .gitignore 规则判定，Lore 无忽略规则 → 恒空
    Ok(serde_json::json!({ "tracked": [], "ignored": ignored, "rule_ignored": [] }))
}

#[tauri::command]
fn add_skip_worktree(project_root: String, path: String) -> Result<(), String> {
    eprintln!("[debug] add_skip_worktree(root={}, path={})", project_root, path);
    validate_entry(&path)?;
    let root = PathBuf::from(&project_root);
    let normalized = path.trim_end_matches('/').to_string();
    let mut list = keinestamp::hidden::load_hidden(&root);
    if !list.iter().any(|h| *h == normalized) {
        list.push(normalized);
    }
    map_err(keinestamp::hidden::save_hidden(&root, &list))
}

#[tauri::command]
fn remove_ignore(project_root: String, entry: String) -> Result<(), String> {
    eprintln!("[debug] remove_ignore(root={}, entry={})", project_root, entry);
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    let needle = entry.trim_end_matches('/').to_string();
    let list = keinestamp::hidden::load_hidden(&root);
    let kept: Vec<String> = list
        .into_iter()
        .filter(|h| *h != needle)
        .collect();
    map_err(keinestamp::hidden::save_hidden(&root, &kept))
}

fn validate_entry(entry: &str) -> Result<(), String> {
    let entry_path = std::path::Path::new(entry);
    if entry.trim().is_empty() || entry_path.is_absolute() {
        return Err("invalid entry".into());
    }
    for comp in entry_path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            return Err("invalid entry path".into());
        }
    }
    Ok(())
}

#[tauri::command]
fn add_to_gitignore(project_root: String, entry: String) -> Result<(), String> {
    eprintln!("[debug] add_to_gitignore(root={}, entry={})", project_root, entry);
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    let normalized = entry.trim_end_matches('/').to_string();
    let mut list = keinestamp::hidden::load_hidden(&root);
    if !list.iter().any(|h| *h == normalized) {
        list.push(normalized);
    }
    map_err(keinestamp::hidden::save_hidden(&root, &list))
}

#[tauri::command]
async fn rollback(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] rollback({}, {})", path, &entry_id[..7.min(entry_id.len())]);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        // Lore 回退 = branch reset，Lore 拒绝 dirty/staged 而非产生冲突 →
        // 错误契约保持 v1（无冲突文件时返回普通错误，CONFLICT| 分支不再触发）
        map_err(keinestamp::lore_project::rollback(&meta.project_root, &entry_id, &hidden))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_state(path: String) -> Result<serde_json::Value, String> {
    eprintln!("[debug] get_rollback_state({})", path);
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    let st = map_err(keinestamp::lore_project::rollback_state(&meta.project_root))?;
    let head_matches = match &st.revision {
        Some(rp) => keinestamp::lore_backend::current_revision(&meta.project_root)
            .map(|cur| cur == *rp)
            .unwrap_or(false),
        None => true,
    };
    Ok(serde_json::json!({
        "rollback_point": st.revision,
        "head_matches": head_matches,
    }))
}

#[tauri::command]
async fn restore_rollback(path: String) -> Result<(), String> {
    eprintln!("[debug] restore_rollback({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        map_err(keinestamp::lore_project::undo_rollback(&meta.project_root, &hidden))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_conflicts(path: String) -> Result<Vec<String>, String> {
    eprintln!("[debug] get_rollback_conflicts({})", path);
    // Lore 无 git 式冲突列表概念：回退冲突由 `revision revert resolve mine/theirs` 处理
    // （Task 9 决定 UI）；保留签名，返回空数组
    let _ = path;
    Ok(vec![])
}

#[tauri::command]
fn keep_rollback_file(path: String, file: String) -> Result<(), String> {
    eprintln!("[debug] keep_rollback_file({}, {})", path, file);
    let _ = (path, file);
    Ok(())
}

#[tauri::command]
async fn resolve_rollback_conflicts(path: String, choices: Vec<Vec<String>>) -> Result<(), String> {
    eprintln!("[debug] resolve_rollback_conflicts({}, {} files)", path, choices.len());
    let _ = choices;
    let _ = path;
    Ok(())
}

#[tauri::command]
fn status(path: String) -> Result<StatusResult, String> {
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
    let files = map_err(keinestamp::lore_project::changed_files(&meta.project_root, &hidden))?;
    let clean = files.is_empty();
    // 输出格式与 CLI status 一致（中文标签，state 码 + 路径）
    let output = if clean {
        String::new()
    } else {
        let mut s = String::from("未提交的修改：\n");
        for f in &files {
            s.push_str(&format!("  {} {}\n", state_code(&f.state), f.path));
        }
        s
    };
    Ok(StatusResult { clean, output })
}

#[tauri::command]
fn log(path: String, count: Option<usize>) -> Result<Vec<LogEntry>, String> {
    eprintln!("[debug] log({}, count={:?})", path, count);
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    let hist = map_err(keinestamp::lore_backend::revision_history(&meta.project_root))?;
    let c = count.unwrap_or(20);
    let entries: Vec<LogEntry> = hist
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
        .take(c)
        .map(|r| LogEntry {
            hash: r.id[..7.min(r.id.len())].to_string(),
            date: format_lore_date(&r.date),
            message: r.message.clone(),
        })
        .collect();
    eprintln!("[debug] log → {} entries (showing {})", entries.len(), c);
    Ok(entries)
}

#[tauri::command]
fn read_config(path: String) -> Result<keinestamp::config::MetaConfig, String> {
    eprintln!("[debug] read_config({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    map_err(keinestamp::config::read_meta(&meta_path))
}

#[tauri::command]
fn update_config(
    path: String,
    key: String,
    value: Option<String>,
) -> Result<(), String> {
    eprintln!("[debug] update_config({}, key={:?}, value={:?})", path, key, value);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let mut meta = map_err(keinestamp::config::read_meta(&meta_path))?;

    match key.as_str() {
        "access_token" => {
            meta.access_token = value;
        }
        "site_name" => {
            if meta.site_locked {
                return Err("站点名已锁定，部署后无法修改".to_string());
            }
            let v = value.filter(|s| !s.is_empty());
            if let Some(name) = &v {
                map_err(keinestamp::config::validate_site_name(name))?;
            }
            meta.site_name = v;
        }
        _ => return Err(format!("unknown config key: {key}. Available: access_token, site_name")),
    }

    map_err(keinestamp::config::write_meta(&meta_path, &meta))
}

#[tauri::command]
fn export_entry(path: String, entry_id: String, output_dir: String) -> Result<(), String> {
    eprintln!("[debug] export_entry({}, entry_id={}, output_dir={})", path, entry_id, output_dir);
    map_err(keinestamp::main_helpers::export_entry(
        Path::new(&path),
        &entry_id,
        Path::new(&output_dir),
    ))
}

#[tauri::command]
fn get_timeline(path: String) -> Result<keinestamp::timeline::Timeline, String> {
    eprintln!("[debug] get_timeline({})", path);
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    // v2：timeline.json 位于主仓库 web/ 下（web/timeline.json）
    let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
    match keinestamp::timeline::read_timeline(&timeline_path) {
        Ok(mut t) => {
            for entry in &mut t.entries {
                if !entry.preview.is_empty() {
                    let abs = meta.web_root.join(&entry.preview);
                    if abs.exists() {
                        entry.preview = abs.to_string_lossy().to_string();
                    } else {
                        entry.preview = String::new();
                    }
                }
            }
            eprintln!("[debug] get_timeline → {} entries", t.entries.len());
            Ok(t)
        }
        Err(_) => Ok(keinestamp::timeline::Timeline {
            project: String::new(),
            entries: vec![],
            rollback_marker: None,
        }),
    }
}

#[tauri::command]
fn get_tracked_files(path: String) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // v2：web/ 为部署产物目录（主仓库内），不计入文件列表
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "web" {
                continue;
            }
            let rel = if prefix.is_empty() { name.to_string() } else { format!("{}/{}", prefix, name) };
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), &rel, files)?;
            } else {
                files.push(rel);
            }
        }
        Ok(())
    }
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    let mut files = vec![];
    map_err(walk(&meta.project_root, "", &mut files).map_err(|e| anyhow::anyhow!("{}", e)))?;
    Ok(files)
}

#[tauri::command]
fn get_recent() -> Vec<keinestamp::recent::RecentEntry> {
    keinestamp::recent::RecentProjects::load().entries
}

#[tauri::command]
fn add_recent(path: String) {
    eprintln!("[debug] add_recent({})", path);
    keinestamp::recent::RecentProjects::add_path(Path::new(&path));
}

#[tauri::command]
fn remove_recent(path: String) {
    eprintln!("[debug] remove_recent({})", path);
    keinestamp::recent::RecentProjects::remove_path(Path::new(&path));
}

#[tauri::command]
fn reorder_recent(paths: Vec<String>) -> Vec<keinestamp::recent::RecentEntry> {
    eprintln!("[debug] reorder_recent({:?})", paths);
    keinestamp::recent::RecentProjects::reorder_paths(&paths);
    keinestamp::recent::RecentProjects::load().entries
}

#[tauri::command]
async fn set_entry_milestone(path: String, entry_id: String, milestone: bool) -> Result<(), String> {
    eprintln!("[debug] set_entry_milestone({}, entry_id={}, milestone={})", path, entry_id, milestone);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        // milestone 权威 = revision metadata，但 `lore revision metadata set` 只作用于
        // staged state（无 --revision 参数，实测报错），已提交 revision 无法回写 →
        // 方案 B：写 timeline.json 展示缓存的 milestone 字段（rebuild_cache 读取
        // metadata，下一记录后 milestone 恢复默认；Task 9 处理 metadata 方案）
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        map_err(keinestamp::timeline::set_entry_milestone(&timeline_path, &entry_id, milestone))?;

        let timeline = map_err(keinestamp::timeline::read_timeline(&timeline_path))?;
        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        map_err(keinestamp::html_gen::generate_index_html(
            &meta.web_root,
            &meta.project_root,
            &timeline,
            &project_name,
        ))?;

        // v1 行为保留：web 产物重新提交（kind=web）
        let _ = keinestamp::lore_backend::commit_web(&meta.project_root);

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_entry(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] delete_entry({}, entry_id={})", path, entry_id);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);

        map_err(keinestamp::timeline::remove_entry(&timeline_path, &entry_id))?;

        let timeline = map_err(keinestamp::timeline::read_timeline(&timeline_path))?;
        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        map_err(keinestamp::html_gen::generate_index_html(
            &meta.web_root,
            &meta.project_root,
            &timeline,
            &project_name,
        ))?;

        let _ = keinestamp::lore_backend::commit_web(&meta.project_root);

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_project_log(path: String, count: Option<usize>) -> Result<Vec<String>, String> {
    eprintln!("[debug] get_project_log({}, count={:?})", path, count);
    let p = PathBuf::from(&path);
    let c = count.unwrap_or(20);
    let meta = read_meta(&p)?;
    let hist = map_err(keinestamp::lore_backend::revision_history(&meta.project_root))?;
    Ok(hist
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
        .take(c)
        .map(|r| format!("{}  {}", &r.id[..7.min(r.id.len())], r.message))
        .collect())
}

#[tauri::command]
fn get_file_diff(path: String, entry_id: String, file_path: String) -> Result<String, String> {
    eprintln!("[debug] get_file_diff({}, entry={}, file={})", path, &entry_id[..7.min(entry_id.len())], file_path);
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    map_err(keinestamp::lore_project::get_file_diff(&meta.project_root, &entry_id, &file_path))
}

#[derive(Clone, serde::Serialize)]
struct BlenderProgress {
    phase: String,
    message: String,
    percent: i32,
    bytes_downloaded: u64,
    bytes_total: u64,
}

#[tauri::command]
async fn download_blender(app: tauri::AppHandle, version: Option<String>) -> Result<String, String> {
    eprintln!("[debug] download_blender(version={:?})", version);
    use tauri::Emitter;
    let version_clone = version.clone();
    let app_clone = app.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let mut progress_tx = |p: keinestamp::blender_download::DownloadProgress| {
            let _ = app_clone.emit("blender-download-progress", BlenderProgress {
                phase: p.phase.clone(),
                message: p.message.clone(),
                percent: p.percent,
                bytes_downloaded: p.bytes_downloaded,
                bytes_total: p.bytes_total,
            });
        };
        keinestamp::blender_download::download_blender_with_progress(version_clone.as_deref(), &mut progress_tx)
            .map(|p| p.to_string_lossy().to_string())
    });
    handle.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_blender_versions() -> Vec<String> {
    eprintln!("[debug] refresh_blender_versions: fetching from mirrors");
    let handle = tokio::task::spawn_blocking(|| {
        keinestamp::blender_download::fetch_blender_versions()
    });
    let fetched = handle.await.unwrap_or(Ok(Vec::new()));
    if let Ok(versions) = fetched {
        if !versions.is_empty() {
            keinestamp::app_config::set_blender_versions(&versions);
            return versions;
        }
    }
    let cached = keinestamp::app_config::get_blender_versions();
    if !cached.is_empty() {
        cached
    } else {
        keinestamp::blender_download::list_blender_versions()
    }
}

#[tauri::command]
fn list_installed_blender_versions() -> Vec<String> {
    eprintln!("[debug] list_installed_blender_versions()");
    keinestamp::blender_download::list_installed_blender_versions()
}

#[tauri::command]
fn get_cached_blender_versions() -> Vec<String> {
    keinestamp::app_config::get_blender_versions()
}

#[tauri::command]
fn get_blender_path() -> String {
    eprintln!("[debug] get_blender_path()");
    let result = keinestamp::app_config::get_blender_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    eprintln!("[debug] get_blender_path → {:?}", result);
    result
}

#[tauri::command]
fn set_blender_path(path: String) {
    eprintln!("[debug] set_blender_path({:?})", path);
    keinestamp::app_config::set_blender_path(&path);
}

#[tauri::command]
fn get_deploy_token() -> String {
    eprintln!("[debug] get_deploy_token()");
    keinestamp::app_config::get_deploy_token().unwrap_or_default()
}

#[tauri::command]
fn set_deploy_token(token: String) {
    eprintln!("[debug] set_deploy_token()");
    keinestamp::app_config::set_deploy_token(&token);
}

#[tauri::command]
fn get_auto_connect() -> bool {
    eprintln!("[debug] get_auto_connect()");
    keinestamp::app_config::get_auto_connect()
}

#[tauri::command]
fn set_auto_connect(enabled: bool) {
    eprintln!("[debug] set_auto_connect({})", enabled);
    keinestamp::app_config::set_auto_connect(enabled);
}

#[tauri::command]
fn get_close_action() -> Option<bool> {
    keinestamp::app_config::get_close_to_tray()
}

#[tauri::command]
fn set_close_action(close_to_tray: Option<bool>) {
    eprintln!("[debug] set_close_action({:?})", close_to_tray);
    keinestamp::app_config::set_close_to_tray(close_to_tray);
}

#[tauri::command]
fn resolve_close_action(app: tauri::AppHandle, close_to_tray: bool) {
    eprintln!("[debug] resolve_close_action({})", close_to_tray);
    keinestamp::app_config::set_close_to_tray(Some(close_to_tray));
    if close_to_tray {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
        }
    } else {
        app.exit(0);
    }
}

#[tauri::command]
fn get_deploy_server_url() -> String {
    eprintln!("[debug] get_deploy_server_url()");
    keinestamp::app_config::get_deploy_server_url().unwrap_or_default()
}

#[tauri::command]
fn set_deploy_server_url(url: String) {
    eprintln!("[debug] set_deploy_server_url()");
    keinestamp::app_config::set_deploy_server_url(&url);
}

static LOCAL_SERVERS: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();

#[tauri::command]
fn open_web_timeline(path: String) -> Result<(), String> {
    eprintln!("[debug] open_web_timeline({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let html_path = meta.web_root.join("index.html");
    if !html_path.exists() {
        return Err("index.html not found — commit first to generate the timeline".to_string());
    }
    let key = meta.web_root.to_string_lossy().to_string();
    let wr = meta.web_root.clone();
    let port = {
        let map = LOCAL_SERVERS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = map.lock().map_err(|e| e.to_string())?;
        let reuse = match guard.get(&key) {
            Some(p) => {
                let alive = format!("127.0.0.1:{}", p)
                    .parse::<std::net::SocketAddr>()
                    .map(|a| std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(300)).is_ok())
                    .unwrap_or(false);
                if alive {
                    Some(*p)
                } else {
                    guard.remove(&key);
                    None
                }
            }
            None => None,
        };
        if let Some(p) = reuse {
            p
        } else {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = keinestamp::serve::serve_local(&wr, tx);
            });
            let p = rx.recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| "timeline server failed to start".to_string())?;
            guard.insert(key, p);
            p
        }
    };
    let url = format!("http://127.0.0.1:{}/", port);
    #[cfg(target_os = "windows")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("cmd")).args(["/c", "start", &url]).spawn();
    #[cfg(target_os = "macos")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("open")).arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open")).arg(&url).spawn();
    result.map_err(|e| format!("failed to open timeline: {}", e))?;
    Ok(())
}

#[tauri::command]
fn open_site(url: String) -> Result<(), String> {
    eprintln!("[debug] open_site({})", url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("invalid URL, must start with http:// or https://".to_string());
    }
    #[cfg(target_os = "windows")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("cmd")).args(["/c", "start", &url]).spawn();
    #[cfg(target_os = "macos")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("open")).arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open")).arg(&url).spawn();
    result.map_err(|e| format!("failed to open site: {}", e))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let open_item = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .tooltip("Keinestamp（左键打开，右键菜单）")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        if let Some(win) = tray.app_handle().get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                match keinestamp::app_config::get_close_to_tray() {
                    Some(true) => {
                        let _ = window.hide();
                    }
                    Some(false) => {
                        app.exit(0);
                    }
                    None => {
                        let _ = window.emit("close-requested", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            detect,
            init_project,
            record,
            push,
            deploy,
            test_connection,
            get_deploy_status,
            pull,
            has_remote,
            rollback,
            get_rollback_state,
            restore_rollback,
            get_rollback_conflicts,
            keep_rollback_file,
            resolve_rollback_conflicts,
            status,
            log,
            read_config,
            update_config,
            export_entry,
            get_timeline,
            get_recent,
            add_recent,
            remove_recent,
            reorder_recent,
            open_folder,
            add_to_gitignore,
            get_ignore_info,
            add_skip_worktree,
            remove_ignore,
            open_web_timeline,
            open_site,
            delete_entry,
            set_entry_milestone,
            get_project_log,
            get_tracked_files,
            get_file_diff,
            download_blender,
            refresh_blender_versions,
            get_cached_blender_versions,
            list_installed_blender_versions,
            get_blender_path,
            set_blender_path,
            get_deploy_token,
            set_deploy_token,
            get_deploy_server_url,
            set_deploy_server_url,
            get_auto_connect,
            set_auto_connect,
            get_close_action,
            set_close_action,
            resolve_close_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


