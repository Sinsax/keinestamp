use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;
use tauri::Manager;

fn map_err<T>(r: anyhow::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
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
    use tauri::Emitter;
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
        let meta_path = keinestamp::config::meta_path(&p);
        let meta: keinestamp::config::MetaConfig = map_err(keinestamp::config::read_meta(&meta_path))?;
        let skip_preview = no_preview.unwrap_or(false);

        map_err(keinestamp::git_project::ensure_git_user(&meta.project_root))?;
        map_err(keinestamp::git_project::lfs_auto_configure(&meta.project_root))?;
        // 回退状态下记录 = 顶替回退点及之后的记录（HEAD 移回回退记录的父提交再提交）
        let rollback_point = map_err(keinestamp::git_project::get_rollback_point_hash(&meta.project_root))?;
        if let Some(ref point) = rollback_point {
            eprintln!("[debug] record: rollback state, reset --soft to {}^", point);
            map_err(keinestamp::git_project::reset_soft_to(&meta.project_root, &format!("{}^", point)))?;
        }
        let hash: String = match files {
            Some(f) => map_err(keinestamp::git_project::add_and_commit_files(&meta.project_root, &f, &message))?,
            None => map_err(keinestamp::git_project::add_and_commit(&meta.project_root, &message))?,
        };
        if let Some(ref point) = rollback_point {
            map_err(keinestamp::timeline::remove_entries_after(&meta.web_root.join("timeline.json"), point))?;
        }
        let _ = keinestamp::git_project::clear_rollback(&meta.project_root);
        let short_hash = hash[..7].to_string();

        let changed_files: Vec<String> = map_err(keinestamp::git_helpers::get_changed_files(&meta.project_root, &hash))?;
        let file_status: Vec<String> = map_err(keinestamp::git_helpers::get_changed_files_status(&meta.project_root, &hash))?;

        let timeline_path = meta.web_root.join("timeline.json");

        let blender_path_buf = if !skip_preview && keinestamp::git_helpers::has_blend_files(&changed_files) {
            emit("blender_prepare", "准备 Blender...");
            match keinestamp::blender_download::ensure_blender() {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("[debug] commit: Blender auto-download failed: {}, trying PATH", e);
                    keinestamp::app_config::get_blender_path()
                }
            }
        } else {
            keinestamp::app_config::get_blender_path()
        };
        let blender_path: Option<&Path> = blender_path_buf.as_deref();

        let preview_path = if !skip_preview && keinestamp::git_helpers::has_blend_files(&changed_files) {
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
                        eprintln!("[debug] commit: GLB export skipped: {e}");
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

        let author = keinestamp::git_helpers::get_git_author(&meta.project_root);
        let entry = keinestamp::timeline::TimelineEntry {
            id: hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
            author,
            message,
            files: changed_files,
            file_status,
            preview: preview_path.unwrap_or_default(),
            tag,
            milestone: false,
        };

        map_err(keinestamp::timeline::append_entry(&timeline_path, entry))?;

        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut timeline: keinestamp::timeline::Timeline = map_err(keinestamp::timeline::read_timeline(&timeline_path))?;
        timeline.project = project_name.clone();
        map_err(keinestamp::timeline::write_timeline(&timeline_path, &timeline))?;

        emit("html", "生成页面...");
        map_err(keinestamp::html_gen::generate_index_html(
            &meta.web_root,
            &meta.project_root,
            &timeline,
            &project_name,
        ))?;

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
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        eprintln!("[debug] push: project_root={}", meta.project_root.display());

        if keinestamp::git_project::has_remote(&meta.project_root) {
            map_err(keinestamp::git_project::push(&meta.project_root))?;
        } else {
            eprintln!("[debug] push: no git remote, skipping git push");
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
        let local = keinestamp::git_web::get_web_commit_count(&meta.web_root);
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
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        eprintln!("[debug] pull: project_root={}", meta.project_root.display());

        if keinestamp::git_project::has_remote(&meta.project_root) {
            map_err(keinestamp::git_project::pull(&meta.project_root))?;
        } else {
            eprintln!("[debug] pull: no git remote, skipping");
        }

        eprintln!("[debug] pull → ok");
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn has_remote(path: String) -> bool {
    eprintln!("[debug] has_remote({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let result = match keinestamp::config::read_meta(&meta_path) {
        Ok(meta) => {
            keinestamp::git_project::has_remote(&meta.project_root)
        }
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

fn git_ls(project_root: &str, args: &[&str]) -> Result<Vec<String>, String> {
    let output = keinestamp::process_util::hide_console(std::process::Command::new("git"))
        .args(["-c", "core.quotepath=false", "ls-files"])
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("failed to run git ls-files: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git ls-files failed: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .map(|s| s.trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

#[tauri::command]
fn get_ignore_info(project_root: String, files: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    eprintln!("[debug] get_ignore_info({})", project_root);
    let tracked = git_ls(&project_root, &[])?;
    let mut ignored = git_ls(&project_root, &["-o", "-i", "--exclude-standard"])?;
    let verbose = git_ls(&project_root, &["-v"])?;
    for line in verbose {
        if let Some(rest) = line.strip_prefix('S') {
            let p = rest.trim_start();
            if !p.is_empty() && !ignored.contains(&p.to_string()) {
                ignored.push(p.to_string());
            }
        }
    }
    // 传入的 status 文件 + 全部 tracked 文件中命中 .gitignore 规则的路径
    // （含已跟踪文件的删除/修改差异；tracked 全量检查保证前端隐藏稳定，不随传入列表波动）
    let mut check_files: Vec<String> = tracked.clone();
    if let Some(fs) = files {
        check_files.extend(fs);
    }
    check_files.sort();
    check_files.dedup();
    let rule_ignored: Vec<String> = keinestamp::git_project::check_ignore(std::path::Path::new(&project_root), &check_files)
        .unwrap_or_default();
    Ok(serde_json::json!({ "tracked": tracked, "ignored": ignored, "rule_ignored": rule_ignored }))
}

fn git_update_index(project_root: &str, args: &[&str]) -> Result<(), String> {
    let output = keinestamp::process_util::hide_console(std::process::Command::new("git"))
        .args(["update-index"])
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("failed to run git update-index: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git update-index failed: {}", stderr.trim()));
    }
    Ok(())
}

#[tauri::command]
fn add_skip_worktree(project_root: String, path: String) -> Result<(), String> {
    eprintln!("[debug] add_skip_worktree(root={}, path={})", project_root, path);
    validate_entry(&path)?;
    let tracked = git_ls(&project_root, &[])?;
    let normalized = path.trim_end_matches('/');
    if !tracked.iter().any(|t| t == normalized) {
        return Err("仅已跟踪文件支持本地忽略".into());
    }
    git_update_index(&project_root, &["--skip-worktree", &normalized])
}

fn rule_matches(line: &str, needle: &str) -> bool {
    let raw = line.trim_end();
    let base = raw.trim_end_matches('/');
    if base == needle {
        return true;
    }
    if raw.ends_with('/') {
        let dir = base;
        if needle.starts_with(&format!("{}/", dir)) {
            return true;
        }
    }
    if let Some(line_ext) = base.strip_prefix("*.") {
        if let Some(dot) = needle.rfind('.') {
            if &needle[dot + 1..] == line_ext {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
fn remove_ignore(project_root: String, entry: String) -> Result<(), String> {
    eprintln!("[debug] remove_ignore(root={}, entry={})", project_root, entry);
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    let needle = entry.trim_end_matches('/');
    let content = read_gitignore(&root);
    let mut kept: Vec<&str> = Vec::new();
    let mut removed = false;
    for line in content.lines() {
        if rule_matches(line, needle) {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    if removed {
        let mut out = kept.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        write_gitignore(&root, &out)?;
    }
    let verbose = git_ls(&project_root, &["-v"])?;
    let skip_targets: Vec<String> = verbose
        .iter()
        .filter_map(|l| l.strip_prefix('S').map(|s| s.trim_start().to_string()))
        .filter(|p| p == needle || p.starts_with(&format!("{}/", needle)))
        .collect();
    for target in skip_targets {
        let _ = git_update_index(&project_root, &["--no-skip-worktree", &target]);
    }
    Ok(())
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

fn read_gitignore(root: &Path) -> String {
    std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default()
}

fn write_gitignore(root: &Path, content: &str) -> Result<(), String> {
    let gitignore = root.join(".gitignore");
    let tmp = gitignore.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("failed to write gitignore: {}", e))?;
    std::fs::rename(&tmp, &gitignore).map_err(|e| format!("failed to save gitignore: {}", e))?;
    Ok(())
}

#[tauri::command]
fn add_to_gitignore(project_root: String, entry: String) -> Result<(), String> {
    eprintln!("[debug] add_to_gitignore(root={}, entry={})", project_root, entry);
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    if !entry.ends_with('/') {
        let tracked = git_ls(&project_root, &[])?;
        let rule_path = entry.trim_end_matches('/');
        if tracked.iter().any(|t| t == rule_path || t.starts_with(&format!("{}/", rule_path))) {
            return Err("已跟踪文件无法忽略".into());
        }
    }
    let mut content = read_gitignore(&root);
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&entry);
    content.push('\n');
    write_gitignore(&root, &content)
}

#[tauri::command]
async fn rollback(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] rollback({}, {})", path, &entry_id[..7.min(entry_id.len())]);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        match keinestamp::git_project::rollback(&meta.project_root, &entry_id) {
            Ok(_) => Ok(()),
            Err(e) => {
                if let Ok(files) = keinestamp::git_project::get_rollback_conflicts(&meta.project_root) {
                    if !files.is_empty() {
                        return Err(format!("CONFLICT|{}", files.join("\n")));
                    }
                }
                Err(e.to_string())
            }
        }
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_state(path: String) -> Result<serde_json::Value, String> {
    eprintln!("[debug] get_rollback_state({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let st = map_err(keinestamp::git_project::get_rollback_state(&meta.project_root))?;
    Ok(serde_json::json!({
        "rollback_point": st.rollback_point,
        "head_matches": st.head_matches,
    }))
}

#[tauri::command]
async fn restore_rollback(path: String) -> Result<(), String> {
    eprintln!("[debug] restore_rollback({})", path);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        match keinestamp::git_project::restore_rollback(&meta.project_root) {
            Ok(_) => Ok(()),
            Err(e) => {
                if let Ok(files) = keinestamp::git_project::get_rollback_conflicts(&meta.project_root) {
                    if !files.is_empty() {
                        return Err(format!("CONFLICT|{}", files.join("\n")));
                    }
                }
                Err(e.to_string())
            }
        }
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_conflicts(path: String) -> Result<Vec<String>, String> {
    eprintln!("[debug] get_rollback_conflicts({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    map_err(keinestamp::git_project::get_rollback_conflicts(&meta.project_root))
}

#[tauri::command]
fn keep_rollback_file(path: String, file: String) -> Result<(), String> {
    eprintln!("[debug] keep_rollback_file({}, {})", path, file);
    validate_entry(&file)?;
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    map_err(keinestamp::git_project::keep_rollback_file(&meta.project_root, &file))
}

#[tauri::command]
async fn resolve_rollback_conflicts(path: String, choices: Vec<Vec<String>>) -> Result<(), String> {
    eprintln!("[debug] resolve_rollback_conflicts({}, {} files)", path, choices.len());
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        let parsed: Vec<(String, bool)> = choices
            .into_iter()
            .map(|c| {
                let path = c.first().cloned().unwrap_or_default();
                let use_latest = c.get(1).map(|s| s == "latest").unwrap_or(false);
                (path, use_latest)
            })
            .collect();
        map_err(keinestamp::git_project::resolve_rollback_conflicts(&meta.project_root, &parsed))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn status(path: String) -> Result<StatusResult, String> {
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let output = map_err(keinestamp::git_project::status(&meta.project_root))?;
    let clean = output.trim().is_empty();
    Ok(StatusResult { clean, output })
}

#[tauri::command]
fn log(path: String, count: Option<usize>) -> Result<Vec<LogEntry>, String> {
    eprintln!("[debug] log({}, count={:?})", path, count);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = match keinestamp::timeline::read_timeline(&timeline_path) {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };
    let c = count.unwrap_or(20);
    eprintln!("[debug] log → {} entries (showing {})", timeline.entries.len(), c);
    Ok(timeline
        .entries
        .iter()
        .rev()
        .take(c)
        .map(|e| LogEntry {
            hash: e.id[..7.min(e.id.len())].to_string(),
            date: e.timestamp[..19.min(e.timestamp.len())].replace('T', " "),
            message: e.message.clone(),
        })
        .collect())
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
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    let timeline_path = meta.web_root.join("timeline.json");
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
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "web-timeline" {
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
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
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
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        let timeline_path = meta.web_root.join("timeline.json");
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

        let _ = keinestamp::git_web::web_commit_and_push(
            &meta.web_root,
            &format!("milestone {}", &entry_id[..7.min(entry_id.len())]),
            false,
        );

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_entry(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] delete_entry({}, entry_id={})", path, entry_id);
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta_path = keinestamp::config::meta_path(&p);
        let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
        let timeline_path = meta.web_root.join("timeline.json");

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

        let _ = keinestamp::git_web::web_commit_and_push(
            &meta.web_root,
            &format!("delete entry {}", &entry_id[..7.min(entry_id.len())]),
            false,
        );

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_project_log(path: String, count: Option<usize>) -> Result<Vec<String>, String> {
    eprintln!("[debug] get_project_log({}, count={:?})", path, count);
    let p = PathBuf::from(&path);
    let c = count.unwrap_or(20);
    let output = map_err(keinestamp::git_project::log(&p, c))?;
    Ok(output.lines().map(|l| l.to_string()).collect())
}

#[tauri::command]
fn get_file_diff(path: String, entry_id: String, file_path: String) -> Result<String, String> {
    eprintln!("[debug] get_file_diff({}, entry={}, file={})", path, &entry_id[..7.min(entry_id.len())], file_path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;

    let output = keinestamp::process_util::hide_console(std::process::Command::new("git"))
        .args([
            "-c", "core.quotepath=false",
            "show",
            &entry_id,
            "--",
            &file_path,
        ])
        .current_dir(&meta.project_root)
        .output()
        .map_err(|e| format!("git show failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git show error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
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
