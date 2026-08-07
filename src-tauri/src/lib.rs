use serde::Serialize;
use std::path::{Path, PathBuf};

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
    let is_already_init = matches!(
        keinestamp::detect::detect_project(&p),
        Ok(keinestamp::detect::ProjectType::Initialized(_))
    );
    map_err(keinestamp::init::init_project(&p))?;
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    if !is_already_init {
        eprintln!("[debug] init_project: first init, committing config files");
        map_err(keinestamp::git_project::ensure_git_user(&meta.project_root))?;
        map_err(keinestamp::git_project::stage_files_and_commit(
            &meta.project_root,
            &[".gitignore", ".gitattributes"],
            "init",
        ))?;
    }
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
        let hash: String = match files {
            Some(f) => map_err(keinestamp::git_project::add_and_commit_files(&meta.project_root, &f, &message))?,
            None => map_err(keinestamp::git_project::add_and_commit(&meta.project_root, &message))?,
        };
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
            for bf in &changed_files {
                if bf.ends_with(".blend") {
                    let safe_name = bf.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                    let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                    let bpath = meta.project_root.join(bf);
                    emit("blender_export", &format!("导出 GLB: {}", bf));
                    if let Err(e) = keinestamp::blender_render::export_to_glb(&bpath, &glb_config.output_dir, &glb_name, blender_path) {
                        eprintln!("[debug] commit: GLB export skipped: {e}");
                    }
                }
            }
            let _ = keinestamp::blender_render::clean_old_glb_previews(
                &meta.web_root.join("previews_glb"),
                glb_config.max_previews,
            );
            Some(String::new())
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
fn push(path: String) -> Result<(), String> {
    eprintln!("[debug] push({})", path);
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
fn pull(path: String) -> Result<(), String> {
    eprintln!("[debug] pull({})", path);
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
    let result = std::process::Command::new("explorer").arg(&path_str).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&path_str).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&path_str).spawn();
    result.map_err(|e| format!("failed to open folder: {}", e))?;
    Ok(())
}

#[tauri::command]
fn revert(path: String) -> Result<(), String> {
    eprintln!("[debug] revert({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;
    map_err(keinestamp::git_project::revert(&meta.project_root))?;
    eprintln!("[debug] revert: git reset --soft HEAD~1 ok");
    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&timeline_path).ok();
    if let Some(mut tl) = timeline {
        if let Some(removed) = tl.entries.pop() {
            eprintln!("[debug] revert: removed entry {}", &removed.id[..7.min(removed.id.len())]);
            let _ = keinestamp::timeline::write_timeline(&timeline_path, &tl);
            let project_name = meta
                .project_root
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let _ = keinestamp::html_gen::generate_index_html(
                &meta.web_root,
                &meta.project_root,
                &tl,
                &project_name,
            );
            let _ = keinestamp::git_web::web_commit_and_push(
                &meta.web_root,
                &format!("revert {}", &removed.id[..7.min(removed.id.len())]),
                false,
            );
        }
    }
    Ok(())
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
                    entry.preview = abs.to_string_lossy().to_string();
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
fn set_entry_milestone(path: String, entry_id: String, milestone: bool) -> Result<(), String> {
    eprintln!("[debug] set_entry_milestone({}, entry_id={}, milestone={})", path, entry_id, milestone);
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
}

#[tauri::command]
fn delete_entry(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] delete_entry({}, entry_id={})", path, entry_id);
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

    let output = std::process::Command::new("git")
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

#[tauri::command]
fn get_glb_preview(path: String, entry_id: String, file_path: String) -> Result<String, String> {
    eprintln!("[debug] get_glb_preview({}, entry={}, file={})", path, &entry_id[..7.min(entry_id.len())], file_path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    let meta = map_err(keinestamp::config::read_meta(&meta_path))?;

    if !file_path.ends_with(".blend") {
        return Err("not a .blend file".to_string());
    }

    let short_hash = &entry_id[..7.min(entry_id.len())];
    let safe_name = file_path.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
    let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));

    let glb_dir = meta.web_root.join("previews_glb");
    let glb_path = glb_dir.join(format!("{}.glb", glb_name));

    if glb_path.exists() {
        return Ok(glb_path.to_string_lossy().to_string());
    }

    let temp_dir = std::env::temp_dir().join(format!("keinestamp_glb_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
    let temp_blend = temp_dir.join(&safe_name);

    let extract_output = std::process::Command::new("git")
        .args([
            "-c", "core.quotepath=false",
            "show",
            &format!("{}:{}", entry_id, file_path),
        ])
        .current_dir(&meta.project_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("git show extraction failed: {e}"))?;

    if !extract_output.status.success() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        let stderr = String::from_utf8_lossy(&extract_output.stderr);
        return Err(format!("git show error extracting {}: {}", file_path, stderr.trim()));
    }

    std::fs::write(&temp_blend, &extract_output.stdout)
        .map_err(|e| format!("failed to write temp blend file: {e}"))?;

    std::fs::create_dir_all(&glb_dir).map_err(|e| format!("failed to create glb dir: {e}"))?;

    let render_config = keinestamp::blender_render::RenderConfig {
        output_dir: glb_dir.clone(),
        ..Default::default()
    };

    let blender_path_buf = match keinestamp::blender_download::ensure_blender() {
        Ok(path) => Some(path),
        Err(e) => {
            eprintln!("Blender auto-download failed: {}, trying PATH", e);
            keinestamp::app_config::get_blender_path()
        }
    };
    let blender_path: Option<&Path> = blender_path_buf.as_deref();
    let result = keinestamp::blender_render::export_to_glb(&temp_blend, &render_config.output_dir, &glb_name, blender_path);

    let _ = std::fs::remove_dir_all(&temp_dir);

    match result {
        Ok(p) => Ok(p.to_string_lossy().to_string()),
        Err(e) => Err(e.to_string()),
    }
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
async fn list_blender_versions() -> Vec<String> {
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
fn get_deploy_server_url() -> String {
    eprintln!("[debug] get_deploy_server_url()");
    keinestamp::app_config::get_deploy_server_url().unwrap_or_default()
}

#[tauri::command]
fn set_deploy_server_url(url: String) {
    eprintln!("[debug] set_deploy_server_url()");
    keinestamp::app_config::set_deploy_server_url(&url);
}

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
    let (tx, rx) = std::sync::mpsc::channel();
    let wr = meta.web_root.clone();
    std::thread::spawn(move || {
        let _ = keinestamp::serve::serve_local(&wr, tx);
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "timeline server failed to start".to_string())?;
    let url = format!("http://127.0.0.1:{}/", port);
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd").args(["/c", "start", &url]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
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
    let result = std::process::Command::new("cmd").args(["/c", "start", &url]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
    result.map_err(|e| format!("failed to open site: {}", e))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|_app| {
            std::thread::spawn(|| {
                let fetched = keinestamp::blender_download::fetch_blender_versions().unwrap_or_default();
                if !fetched.is_empty() {
                    keinestamp::app_config::set_blender_versions(&fetched);
                }
            });
            Ok(())
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
            revert,
            status,
            log,
            read_config,
            update_config,
            export_entry,
            get_timeline,
            get_recent,
            add_recent,
            remove_recent,
            open_folder,
            open_web_timeline,
            open_site,
            delete_entry,
            set_entry_milestone,
            get_project_log,
            get_tracked_files,
            get_file_diff,
            get_glb_preview,
            download_blender,
            list_blender_versions,
            list_installed_blender_versions,
            get_blender_path,
            set_blender_path,
            get_deploy_token,
            set_deploy_token,
            get_deploy_server_url,
            set_deploy_server_url,
            get_auto_connect,
            set_auto_connect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}