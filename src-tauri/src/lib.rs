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
    let pt = map_err(saki_step::detect::detect_project(Path::new(&path)))?;
    let result = match pt {
        saki_step::detect::ProjectType::Fresh => "fresh".to_string(),
        saki_step::detect::ProjectType::Existing => "existing".to_string(),
        saki_step::detect::ProjectType::Initialized(_) => "initialized".to_string(),
    };
    eprintln!("[debug] detect → {}", result);
    Ok(result)
}

#[tauri::command]
fn init_project(path: String) -> Result<saki_step::config::MetaConfig, String> {
    eprintln!("[debug] init_project({})", path);
    let p = PathBuf::from(&path);
    let is_already_init = matches!(
        saki_step::detect::detect_project(&p),
        Ok(saki_step::detect::ProjectType::Initialized(_))
    );
    map_err(saki_step::init::init_project(&p))?;
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    if !is_already_init {
        eprintln!("[debug] init_project: first init, committing config files");
        map_err(saki_step::git_project::ensure_git_user(&meta.project_root))?;
        map_err(saki_step::git_project::stage_files_and_commit(
            &meta.project_root,
            &[".saki-step.toml", ".gitignore", ".gitattributes"],
            "init",
        ))?;
    }
    saki_step::recent::RecentProjects::add_path(&p);
    eprintln!("[debug] init_project → ok, project_root={}", meta.project_root.display());
    map_err(saki_step::config::read_meta(&meta_path))
}

#[derive(Clone, serde::Serialize)]
struct CommitProgress {
    phase: String,
    message: String,
}

#[tauri::command]
async fn record(app: tauri::AppHandle, path: String, message: String, tag: Option<String>, no_preview: Option<bool>) -> Result<String, String> {
    eprintln!("[debug] record({}, message={:?}, tag={:?}, no_preview={})", path, message, tag, no_preview.unwrap_or(false));
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
        let meta_path = saki_step::config::meta_path(&p);
        let meta: saki_step::config::MetaConfig = map_err(saki_step::config::read_meta(&meta_path))?;
        let skip_preview = no_preview.unwrap_or(false);

        map_err(saki_step::git_project::ensure_git_user(&meta.project_root))?;
        map_err(saki_step::git_project::lfs_auto_configure(&meta.project_root))?;
        let hash: String = map_err(saki_step::git_project::add_and_commit(&meta.project_root, &message))?;
        let short_hash = hash[..7].to_string();

        let changed_files: Vec<String> = map_err(saki_step::git_helpers::get_changed_files(&meta.project_root, &hash))?;
        let file_status: Vec<String> = map_err(saki_step::git_helpers::get_changed_files_status(&meta.project_root, &hash))?;

        let timeline_path = meta.web_root.join("timeline.json");

        let blender_path_buf = if !skip_preview && saki_step::git_helpers::has_blend_files(&changed_files) {
            emit("blender_prepare", "准备 Blender...");
            match saki_step::blender_download::ensure_blender() {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("[debug] commit: Blender auto-download failed: {}, trying PATH", e);
                    saki_step::app_config::get_blender_path()
                }
            }
        } else {
            saki_step::app_config::get_blender_path()
        };
        let blender_path: Option<&Path> = blender_path_buf.as_deref();

        let preview_path = if !skip_preview && saki_step::git_helpers::has_blend_files(&changed_files) {
            let glb_config = saki_step::blender_render::RenderConfig {
                output_dir: meta.web_root.join("previews_glb"),
                ..Default::default()
            };
            for bf in &changed_files {
                if bf.ends_with(".blend") {
                    let safe_name = bf.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                    let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                    let bpath = meta.project_root.join(bf);
                    emit("blender_export", &format!("导出 GLB: {}", bf));
                    if let Err(e) = saki_step::blender_render::export_to_glb(&bpath, &glb_config.output_dir, &glb_name, blender_path) {
                        eprintln!("[debug] commit: GLB export skipped: {e}");
                    }
                }
            }
            let _ = saki_step::blender_render::clean_old_glb_previews(
                &meta.web_root.join("previews_glb"),
                glb_config.max_previews,
            );
            Some(String::new())
        } else {
            None
        };

        emit("timeline", "更新时间线...");

        let author = saki_step::git_helpers::get_git_author(&meta.project_root);
        let entry = saki_step::timeline::TimelineEntry {
            id: hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
            author,
            message,
            files: changed_files,
            file_status,
            preview: preview_path.unwrap_or_default(),
            tag,
        };

        map_err(saki_step::timeline::append_entry(&timeline_path, entry))?;

        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut timeline: saki_step::timeline::Timeline = map_err(saki_step::timeline::read_timeline(&timeline_path))?;
        timeline.project = project_name.clone();
        map_err(saki_step::timeline::write_timeline(&timeline_path, &timeline))?;

        emit("html", "生成页面...");
        map_err(saki_step::html_gen::generate_index_html(
            &meta.web_root,
            &meta.project_root,
            &timeline,
            &project_name,
        ))?;

        saki_step::recent::RecentProjects::add_path(&p);
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
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    eprintln!("[debug] push: project_root={}", meta.project_root.display());

    if saki_step::git_project::has_remote(&meta.project_root) {
        map_err(saki_step::git_project::push(&meta.project_root))?;
    } else {
        eprintln!("[debug] push: no git remote, skipping git push");
    }

    eprintln!("[debug] push → ok");
    Ok(())
}

#[tauri::command]
fn deploy(path: String) -> Result<(), String> {
    eprintln!("[debug] deploy({})", path);
    let p = PathBuf::from(&path);
    let meta_p = saki_step::config::meta_path(&p);
    let mut meta = map_err(saki_step::config::read_meta(&meta_p))?;

    if meta.project_id.is_none() {
        meta.project_id = Some(saki_step::config::generate_project_id());
        map_err(saki_step::config::write_meta(&meta_p, &meta))?;
    }

    let server = saki_step::app_config::get_deploy_server_url()
        .or_else(|| meta.web_remote.clone())
        .filter(|s| !s.is_empty());

    if let Some(ref server) = server {
        eprintln!("[debug] deploy: server={}", server);
        let deploy_token = saki_step::app_config::get_deploy_token();
        let project_name = meta.project_root.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let project_id = meta.project_id.as_deref().unwrap_or("");
        map_err(saki_step::push::deploy_to_server(
            &meta.web_root, server, deploy_token.as_deref(), meta.access_token.as_deref(), &project_name, project_id,
        ))?;
        eprintln!("[debug] deploy → ok");
        Ok(())
    } else {
        Err("未设置服务器地址".to_string())
    }
}

#[tauri::command]
fn test_connection(server_url: String, deploy_token: Option<String>) -> Result<bool, String> {
    eprintln!("[debug] test_connection(server_url={:?})", server_url);
    if server_url.is_empty() {
        return Ok(false);
    }
    let health_url = format!("{}/health", server_url.trim_end_matches('/'));
    eprintln!("[debug] test_connection: GET {}", health_url);

    let mut req = ureq::get(&health_url);
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
}

#[tauri::command]
fn pull(path: String) -> Result<(), String> {
    eprintln!("[debug] pull({})", path);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    eprintln!("[debug] pull: project_root={}", meta.project_root.display());

    if saki_step::git_project::has_remote(&meta.project_root) {
        map_err(saki_step::git_project::pull(&meta.project_root))?;
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
    let meta_path = saki_step::config::meta_path(&p);
    let result = match saki_step::config::read_meta(&meta_path) {
        Ok(meta) => {
            saki_step::git_project::has_remote(&meta.project_root)
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
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    map_err(saki_step::git_project::revert(&meta.project_root))?;
    eprintln!("[debug] revert: git reset --soft HEAD~1 ok");
    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = saki_step::timeline::read_timeline(&timeline_path).ok();
    if let Some(mut tl) = timeline {
        if let Some(removed) = tl.entries.pop() {
            eprintln!("[debug] revert: removed entry {}", &removed.id[..7.min(removed.id.len())]);
            let _ = saki_step::timeline::write_timeline(&timeline_path, &tl);
            let project_name = meta
                .project_root
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let _ = saki_step::html_gen::generate_index_html(
                &meta.web_root,
                &meta.project_root,
                &tl,
                &project_name,
            );
            let _ = saki_step::git_web::web_commit_and_push(
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
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let output = map_err(saki_step::git_project::status(&meta.project_root))?;
    let clean = output.trim().is_empty();
    Ok(StatusResult { clean, output })
}

#[tauri::command]
fn log(path: String, count: Option<usize>) -> Result<Vec<LogEntry>, String> {
    eprintln!("[debug] log({}, count={:?})", path, count);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = match saki_step::timeline::read_timeline(&timeline_path) {
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
fn read_config(path: String) -> Result<saki_step::config::MetaConfig, String> {
    eprintln!("[debug] read_config({})", path);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    map_err(saki_step::config::read_meta(&meta_path))
}

#[tauri::command]
fn update_config(
    path: String,
    key: String,
    value: Option<String>,
) -> Result<(), String> {
    eprintln!("[debug] update_config({}, key={:?}, value={:?})", path, key, value);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let mut meta = map_err(saki_step::config::read_meta(&meta_path))?;

    match key.as_str() {
        "web_remote" => {
            meta.web_remote = value.filter(|s| !s.is_empty());
        }
        "access_token" => {
            meta.access_token = value;
        }
        "auto_connect" => {
            let v = value.as_deref().unwrap_or("false");
            meta.auto_connect = v == "true" || v == "1" || v == "yes";
        }
        _ => return Err(format!("unknown config key: {key}. Available: web_remote, access_token, auto_connect")),
    }

    map_err(saki_step::config::write_meta(&meta_path, &meta))
}

#[tauri::command]
fn export_entry(path: String, entry_id: String, output_dir: String) -> Result<(), String> {
    eprintln!("[debug] export_entry({}, entry_id={}, output_dir={})", path, entry_id, output_dir);
    map_err(saki_step::main_helpers::export_entry(
        Path::new(&path),
        &entry_id,
        Path::new(&output_dir),
    ))
}

#[tauri::command]
fn get_timeline(path: String) -> Result<saki_step::timeline::Timeline, String> {
    eprintln!("[debug] get_timeline({})", path);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let timeline_path = meta.web_root.join("timeline.json");
    match saki_step::timeline::read_timeline(&timeline_path) {
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
        Err(_) => Ok(saki_step::timeline::Timeline {
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
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let mut files = vec![];
    map_err(walk(&meta.project_root, "", &mut files).map_err(|e| anyhow::anyhow!("{}", e)))?;
    Ok(files)
}

#[tauri::command]
fn get_recent() -> Vec<saki_step::recent::RecentEntry> {
    saki_step::recent::RecentProjects::load().entries
}

#[tauri::command]
fn add_recent(path: String) {
    eprintln!("[debug] add_recent({})", path);
    saki_step::recent::RecentProjects::add_path(Path::new(&path));
}

#[tauri::command]
fn remove_recent(path: String) {
    eprintln!("[debug] remove_recent({})", path);
    saki_step::recent::RecentProjects::remove_path(Path::new(&path));
}

#[tauri::command]
fn delete_entry(path: String, entry_id: String) -> Result<(), String> {
    eprintln!("[debug] delete_entry({}, entry_id={})", path, entry_id);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let timeline_path = meta.web_root.join("timeline.json");

    map_err(saki_step::timeline::remove_entry(&timeline_path, &entry_id))?;

    let timeline = map_err(saki_step::timeline::read_timeline(&timeline_path))?;
    let project_name = meta
        .project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    map_err(saki_step::html_gen::generate_index_html(
        &meta.web_root,
        &meta.project_root,
        &timeline,
        &project_name,
    ))?;

    let _ = saki_step::git_web::web_commit_and_push(
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
    let output = map_err(saki_step::git_project::log(&p, c))?;
    Ok(output.lines().map(|l| l.to_string()).collect())
}

#[tauri::command]
fn get_file_diff(path: String, entry_id: String, file_path: String) -> Result<String, String> {
    eprintln!("[debug] get_file_diff({}, entry={}, file={})", path, &entry_id[..7.min(entry_id.len())], file_path);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;

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
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;

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

    let temp_dir = std::env::temp_dir().join(format!("saki_glb_{}", std::process::id()));
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

    let render_config = saki_step::blender_render::RenderConfig {
        output_dir: glb_dir.clone(),
        ..Default::default()
    };

    let blender_path_buf = match saki_step::blender_download::ensure_blender() {
        Ok(path) => Some(path),
        Err(e) => {
            eprintln!("Blender auto-download failed: {}, trying PATH", e);
            saki_step::app_config::get_blender_path()
        }
    };
    let blender_path: Option<&Path> = blender_path_buf.as_deref();
    let result = saki_step::blender_render::export_to_glb(&temp_blend, &render_config.output_dir, &glb_name, blender_path);

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
        let mut progress_tx = |p: saki_step::blender_download::DownloadProgress| {
            let _ = app_clone.emit("blender-download-progress", BlenderProgress {
                phase: p.phase.clone(),
                message: p.message.clone(),
                percent: p.percent,
                bytes_downloaded: p.bytes_downloaded,
                bytes_total: p.bytes_total,
            });
        };
        saki_step::blender_download::download_blender_with_progress(version_clone.as_deref(), &mut progress_tx)
            .map(|p| p.to_string_lossy().to_string())
    });
    handle.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_blender_versions() -> Vec<String> {
    let cached = saki_step::app_config::get_blender_versions();
    if !cached.is_empty() {
        cached
    } else {
        saki_step::blender_download::list_blender_versions()
    }
}

#[tauri::command]
fn list_installed_blender_versions() -> Vec<String> {
    eprintln!("[debug] list_installed_blender_versions()");
    saki_step::blender_download::list_installed_blender_versions()
}

#[tauri::command]
fn get_blender_path() -> String {
    eprintln!("[debug] get_blender_path()");
    let result = saki_step::app_config::get_blender_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    eprintln!("[debug] get_blender_path → {:?}", result);
    result
}

#[tauri::command]
fn set_blender_path(path: String) {
    eprintln!("[debug] set_blender_path({:?})", path);
    saki_step::app_config::set_blender_path(&path);
}

#[tauri::command]
fn get_deploy_token() -> String {
    eprintln!("[debug] get_deploy_token()");
    saki_step::app_config::get_deploy_token().unwrap_or_default()
}

#[tauri::command]
fn set_deploy_token(token: String) {
    eprintln!("[debug] set_deploy_token()");
    saki_step::app_config::set_deploy_token(&token);
}

#[tauri::command]
fn get_deploy_server_url() -> String {
    eprintln!("[debug] get_deploy_server_url()");
    saki_step::app_config::get_deploy_server_url().unwrap_or_default()
}

#[tauri::command]
fn set_deploy_server_url(url: String) {
    eprintln!("[debug] set_deploy_server_url()");
    saki_step::app_config::set_deploy_server_url(&url);
}

#[tauri::command]
fn open_web_timeline(path: String) -> Result<(), String> {
    eprintln!("[debug] open_web_timeline({})", path);
    let p = PathBuf::from(&path);
    let meta_path = saki_step::config::meta_path(&p);
    let meta = map_err(saki_step::config::read_meta(&meta_path))?;
    let html_path = meta.web_root.join("index.html");
    if !html_path.exists() {
        return Err("index.html not found — commit first to generate the timeline".to_string());
    }
    let canonical = std::fs::canonicalize(&html_path).unwrap_or(html_path);
    let mut clean = canonical.to_string_lossy().to_string();
    if clean.starts_with(r"\\?\") {
        clean = clean[4..].to_string();
    }
    let url = format!("file:///{}", clean.replace('\\', "/"));
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd").args(["/c", "start", &url]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
    result.map_err(|e| format!("failed to open timeline: {}", e))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|_app| {
            std::thread::spawn(|| {
                let fetched = saki_step::blender_download::fetch_blender_versions().unwrap_or_default();
                if !fetched.is_empty() {
                    saki_step::app_config::set_blender_versions(&fetched);
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
            delete_entry,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}