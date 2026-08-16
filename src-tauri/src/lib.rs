use keinestamp::preview::PreviewKind;
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

#[derive(Serialize)]
pub struct FileDiff {
    pub file: String,
    pub diff: String,
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
    eprintln!(
        "[debug] init_project → ok, project_root={}",
        meta.project_root.display()
    );
    map_err(keinestamp::config::read_meta(&meta_path))
}

#[derive(Clone, serde::Serialize)]
struct CommitProgress {
    phase: String,
    message: String,
}

#[tauri::command]
async fn record(
    app: tauri::AppHandle,
    path: String,
    message: String,
    tag: Option<String>,
    tag_category: Option<String>,
    tags: Option<Vec<String>>,
    no_preview: Option<bool>,
    files: Option<Vec<String>>,
    preview_files: Option<Vec<String>>,
) -> Result<String, String> {
    eprintln!(
        "[debug] record({}, message={:?}, tag={:?}, tag_category={:?}, tags={:?}, no_preview={}, files={:?}, preview_files={:?})",
        path,
        message,
        tag,
        tag_category,
        tags,
        no_preview.unwrap_or(false),
        files,
        preview_files
    );
    let app_clone = app.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let emit = |phase: &str, msg: &str| {
            eprintln!("[debug] record-phase: {} — {}", phase, msg);
            let _ = app_clone.emit(
                "commit-progress",
                CommitProgress {
                    phase: phase.to_string(),
                    message: msg.to_string(),
                },
            );
        };

        emit("git_commit", "记录中...");

        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        let skip_preview = no_preview.unwrap_or(false);

        // 本次记录涉及的文件（Blender 预览判定用；selected 或全量 changed）
        let changed_files: Vec<String> = match &files {
            Some(sel) => sel.clone(),
            None => map_err(keinestamp::lore_project::changed_files(
                &meta.project_root,
                &hidden,
            ))?
            .into_iter()
            .map(|f| f.path)
            .collect(),
        };
        // 预览配置（glb / image / video；用户可在 .keinestamp/preview.toml 覆盖默认映射）
        let preview_cfg = keinestamp::preview::PreviewConfig::load_or_default(&meta.project_root);
        // 预览目标必须在提交前算好：preview_files = Some([])（本次没有任何预览标记）时，
        // 绝不去下载/启动 Blender——旧实现只要“有可预览文件”就 ensure_blender，会在大项目上
        // 表现为记录卡死（Blender 下载/首次启动耗时）。
        let preview_targets: Option<Vec<&String>> = preview_files.as_ref().map(|list| {
            changed_files
                .iter()
                .filter(|f| {
                    list.contains(f)
                        && preview_cfg.kind_of(f) != keinestamp::preview::PreviewKind::None
                })
                .collect()
        });
        let mut preview_sources: Vec<&String> = match &preview_targets {
            Some(targets) => targets.clone(),
            None => changed_files
                .iter()
                .filter(|f| preview_cfg.kind_of(f) != keinestamp::preview::PreviewKind::None)
                .collect(),
        };
        preview_sources.sort();
        preview_sources.dedup();
        let needs_blender = !skip_preview
            && preview_sources
                .iter()
                .any(|f| preview_cfg.kind_of(f) == keinestamp::preview::PreviewKind::Glb);
        let commit_size: u64 = changed_files
            .iter()
            .filter_map(|f| meta.project_root.join(f).metadata().ok())
            .map(|m| m.len())
            .sum();
        emit(
            "git_commit",
            &format!(
                "正在提交 {} 个文件（{:.1} MB）...",
                changed_files.len(),
                commit_size as f64 / 1024.0 / 1024.0
            ),
        );

        // 记录（回退状态下 = 顶替：Lore branch reset 后 HEAD 即回退点，新记录自然顶替）；
        // 成功后清回退标记
        let hash: String = match files {
            Some(f) => map_err(keinestamp::lore_project::record_with_category(
                &meta.project_root,
                &message,
                tag.as_deref(),
                tag_category.as_deref(),
                tags.as_deref(),
                Some(&f),
                &hidden,
            ))?,
            None => map_err(keinestamp::lore_project::record_with_category(
                &meta.project_root,
                &message,
                tag.as_deref(),
                tag_category.as_deref(),
                tags.as_deref(),
                None,
                &hidden,
            ))?,
        };
        // 提交已成功 → 后续步骤（清回退态/时间线重建/HTML 生成）失败只降级为警告，
        // 不把"已提交"误报成"记录失败"（否则用户重试报"无改动可记录"）
        if let Err(e) = keinestamp::lore_project::clear_rollback(&meta.project_root) {
            eprintln!("[debug] record: clear_rollback 失败（降级为警告）: {e}");
        }
        let short_hash = hash[..7.min(hash.len())].to_string();

        let blender_path_buf = if needs_blender {
            emit("blender_prepare", "准备 Blender...");
            match keinestamp::blender_download::ensure_blender() {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!(
                        "[debug] record: Blender auto-download failed: {}, trying PATH",
                        e
                    );
                    keinestamp::app_config::get_blender_path()
                }
            }
        } else {
            // 没有需要 GLB 导出的文件：完全不碰 Blender，避免无谓的下载/启动耗时
            keinestamp::app_config::get_blender_path()
        };
        let blender_path: Option<&Path> = blender_path_buf.as_deref();

        let mut preview_errors: Vec<String> = Vec::new();
        let preview_result: Vec<(String, PreviewKind, String)> = if !skip_preview
            && !preview_sources.is_empty()
        {
            let glb_max_previews = keinestamp::blender_render::RenderConfig::default().max_previews;
            let mut generated: Vec<(PathBuf, PreviewKind, String)> = Vec::new();
            for bf in preview_sources {
                let kind = preview_cfg.kind_of(bf);
                let Some(provider) = keinestamp::preview::PreviewConfig::provider_for(
                    kind,
                    blender_path.map(|p| p.to_path_buf()),
                ) else {
                    let msg = format!("{}: 预览类型 {} 尚未实现", bf, kind.as_str());
                    eprintln!("[debug] record: {msg}");
                    preview_errors.push(msg);
                    continue;
                };
                let safe_name = bf.replace(
                    |c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_',
                    "_",
                );
                let base_name = safe_name
                    .rsplit_once('.')
                    .map(|(base, _)| base.to_string())
                    .unwrap_or_else(|| safe_name.clone());
                let output_name = format!("{}_{}", short_hash, base_name);
                let bpath = meta.project_root.join(bf);
                let output_dir = meta.web_root.join(provider.output_subdir());
                emit("preview_export", &format!("生成预览: {}", bf));
                match provider.generate(&bpath, &output_dir, &output_name) {
                    Ok(Some(path)) => generated.push((path, kind, bf.clone())),
                    Ok(None) => {
                        let msg = format!("{}: 未能生成预览", bf);
                        eprintln!("[debug] record: {msg}");
                        preview_errors.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("{}: {}", bf, e);
                        eprintln!("[debug] record: 预览生成失败（降级为无预览）: {e}");
                        preview_errors.push(msg);
                    }
                }
            }
            let _ = keinestamp::blender_render::clean_old_glb_previews(
                &meta.project_root,
                &meta.web_root.join("previews_glb"),
                glb_max_previews,
            );
            generated
                .into_iter()
                .filter_map(|(path, kind, file)| {
                    path.strip_prefix(&meta.web_root)
                        .ok()
                        .map(|rel| (rel.to_string_lossy().replace('\\', "/"), kind, file))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        emit("timeline", "更新时间线...");

        // 时间线缓存已由 record() 内部 rebuild_cache 重建；补写全部预览路径、类型、开关与失败诊断
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        if let Ok(mut tl) = keinestamp::timeline::read_timeline(&timeline_path) {
            if let Some(e) = tl.entries.iter_mut().find(|e| e.id == hash) {
                e.preview_errors = preview_errors;
                if !preview_result.is_empty() {
                    e.previews = preview_result
                        .iter()
                        .map(|(path, kind, file)| keinestamp::timeline::PreviewFile {
                            file: file.clone(),
                            path: path.clone(),
                            kind: kind.as_str().to_string(),
                            enabled: true,
                        })
                        .collect();
                    let (first_path, first_kind, _) = &preview_result[0];
                    e.preview = first_path.clone();
                    e.preview_type = first_kind.as_str().to_string();
                    e.preview_enabled = true;
                }
            }
            let _ = keinestamp::timeline::write_timeline(&timeline_path, &tl);
        }

        let project_name = meta
            .project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        emit("html", "生成页面...");
        if let Err(e) = keinestamp::timeline::read_timeline(&timeline_path).and_then(|timeline| {
            keinestamp::html_gen::generate_index_html(
                &meta.web_root,
                &meta.project_root,
                &timeline,
                &project_name,
            )
        }) {
            eprintln!("[debug] record: 时间线/HTML 生成失败（降级为警告，提交已成功）: {e}");
        }
        // web 产物版本自增（.keinestamp/ 整体隔离，web 不入库；部署 zip 上传磁盘产物）
        let _ = keinestamp::lore_project::bump_web_version(&meta.project_root);

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
            map_err(keinestamp::lore_backend::run_lore(
                &meta.project_root,
                &["push"],
            ))?;
        } else {
            eprintln!("[debug] push: no lore remote, skipping");
        }

        eprintln!("[debug] push → ok");
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

/// 局域网备份：推送全部本地 revision 到 loreserver（同机/局域网，配置见 app_config lore_server_url）。
#[tauri::command]
async fn backup(path: String) -> Result<(), String> {
    eprintln!("[debug] backup({})", path);
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        if !keinestamp::lore_backend::repo_exists(&p) {
            return Err("项目尚未初始化 lore 仓库".to_string());
        }
        map_err(keinestamp::lore_backend::push_to_server(&p).map(|_| ()))?;
        eprintln!("[debug] backup → ok");
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
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

        let server = keinestamp::app_config::get_deploy_server_url().filter(|s| !s.is_empty());

        if let Some(ref server) = server {
            eprintln!("[debug] deploy: server={}", server);
            let deploy_token = keinestamp::app_config::get_deploy_token();
            let project_name = keinestamp::config::resolve_site_name(&meta);
            let project_id = meta.project_id.as_deref().unwrap_or("");
            map_err(keinestamp::push::deploy_to_server(
                &meta.web_root,
                server,
                deploy_token.as_deref(),
                meta.access_token.as_deref(),
                &project_name,
                project_id,
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
                eprintln!(
                    "[debug] test_connection → {}",
                    if ok { "connected" } else { "failed" }
                );
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
        let local = map_err(keinestamp::lore_project::web_revision_count(
            &meta.project_root,
        ))?;
        if !meta.site_locked {
            eprintln!("[debug] get_deploy_status: not deployed (site_locked=false), skip request");
            return Ok(DeployStatus {
                local,
                remote: None,
            });
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
            map_err(keinestamp::lore_backend::run_lore(
                &meta.project_root,
                &["sync"],
            ))?;
        } else {
            eprintln!("[debug] pull: no lore remote, skipping");
        }

        eprintln!("[debug] pull → ok");
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

/// lore 仓库 remote 判定：`.lore/config.toml` 的 remote_url **值**非空（离线仓库 = 空串，不算有远端）
fn has_remote_impl(project_root: &Path) -> bool {
    keinestamp::lore_backend::has_remote_url(project_root)
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
    let result = keinestamp::process_util::hide_console(std::process::Command::new("explorer"))
        .arg(&path_str)
        .spawn();
    #[cfg(target_os = "macos")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("open"))
        .arg(&path_str)
        .spawn();
    #[cfg(target_os = "linux")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open"))
        .arg(&path_str)
        .spawn();
    result.map_err(|e| format!("failed to open folder: {}", e))?;
    Ok(())
}

#[tauri::command]
fn get_ignore_info(
    project_root: String,
    files: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    // v2：忽略规则存项目级 .loreignore（Lore 原生过滤，status/stage 自动生效）；
    // ignored 返回规则列表供设置界面展示；tracked 概念不存在
    // rule_ignored = 传入文件列表中命中规则者（后端权威匹配，lore 语义 hidden::is_ignored）——
    // 变更面板灰显忽略文件以此为准（前端不再自复制匹配逻辑，消除语义漂移）
    let root = PathBuf::from(&project_root);
    let rules = keinestamp::hidden::load_ignore_rules(&root);
    let rule_ignored: Vec<String> = files
        .unwrap_or_default()
        .into_iter()
        .filter(|f| keinestamp::hidden::is_ignored(f, &rules))
        .collect();
    Ok(serde_json::json!({ "tracked": [], "ignored": rules, "rule_ignored": rule_ignored }))
}

#[tauri::command]
fn add_skip_worktree(project_root: String, path: String) -> Result<(), String> {
    eprintln!(
        "[debug] add_skip_worktree(root={}, path={})",
        project_root, path
    );
    validate_entry(&path)?;
    let root = PathBuf::from(&project_root);
    let normalized = path.trim_end_matches('/').to_string();
    let mut rules = keinestamp::hidden::load_ignore_rules(&root);
    if !rules.iter().any(|r| *r == normalized) {
        rules.push(normalized);
    }
    map_err(keinestamp::hidden::save_ignore_rules(&root, &rules))
}

#[tauri::command]
fn remove_ignore(project_root: String, entry: String) -> Result<(), String> {
    eprintln!(
        "[debug] remove_ignore(root={}, entry={})",
        project_root, entry
    );
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    let needle = entry.trim_end_matches('/').to_string();
    let rules = keinestamp::hidden::load_ignore_rules(&root);
    let kept: Vec<String> = rules.into_iter().filter(|r| *r != needle).collect();
    map_err(keinestamp::hidden::save_ignore_rules(&root, &kept))
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
    eprintln!(
        "[debug] add_to_gitignore(root={}, entry={})",
        project_root, entry
    );
    validate_entry(&entry)?;
    let root = PathBuf::from(&project_root);
    let normalized = entry.trim_end_matches('/').to_string();
    let mut rules = keinestamp::hidden::load_ignore_rules(&root);
    if !rules.iter().any(|r| *r == normalized) {
        rules.push(normalized);
    }
    map_err(keinestamp::hidden::save_ignore_rules(&root, &rules))
}

#[tauri::command]
async fn rollback(path: String, entry_id: String, discard: bool) -> Result<(), String> {
    eprintln!(
        "[debug] rollback({}, {}, discard={})",
        path,
        &entry_id[..7.min(entry_id.len())],
        discard
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        // Lore 回退 = branch reset，Lore 拒绝 dirty/staged 而非产生冲突 →
        // 错误契约保持 v1（无冲突文件时返回普通错误，CONFLICT| 分支不再触发）
        map_err(keinestamp::lore_project::rollback(
            &meta.project_root,
            &entry_id,
            &hidden,
            discard,
        ))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_rollback_state(path: String) -> Result<serde_json::Value, String> {
    // 3 秒轮询高频调用：不打日志（轮询静默，错误由前端展示）
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        match keinestamp::timeline::read_marker(&meta.project_root) {
            Ok(Some(m)) => {
                let head_matches =
                    match keinestamp::lore_backend::current_revision(&meta.project_root) {
                        Ok(cur) => cur == m.rollback_point, // current == X-1（回退后 HEAD）
                        Err(_) => {
                            // 读取失败（超时/锁竞争等瞬态错误）：保守保留回退态（不清 marker/备份），
                            // 视为仍处于回退态，等待下次成功轮询再判定——避免一次瞬态失败销毁回退前未提交改动
                            eprintln!(
                                "[debug] rollback state: current_revision 读取失败，保守保留回退态"
                            );
                            true
                        }
                    };
                if !head_matches {
                    if keinestamp::lore_project::rollback_operation_in_progress() {
                        // 正常回退/恢复过程中 HEAD 会先移动、最后才清 marker；此时不能把
                        // 中间窗口当成异常残留自动清理（否则会并发删除备份/标记，甚至造成卡顿）
                        eprintln!("[debug] rollback marker 不一致（HEAD 已前进），但回退/恢复操作进行中，跳过自动清理");
                        return Ok(serde_json::json!({
                            "rollback_point": m.superseded_point,
                            "head_matches": false,
                            "deleted_files": m.deleted_files,
                            "change_files": m.change_files,
                            "modified_files": [],
                        }));
                    } else {
                        // 只有确认 HEAD 已前进（current_revision 读取成功且 != rollback_point）才清理——
                        // 半状态残留（HEAD 已前进但 marker 未清——异常退出/恢复失败等）：
                        // 自动清理（marker + 备份），避免"恢复按钮 disabled + 回退态残留"卡死
                        eprintln!("[debug] rollback marker 不一致（HEAD 已前进），自动清理回退态");
                        let _ = keinestamp::lore_project::clear_rollback(&meta.project_root);
                        return Ok(serde_json::json!({
                            "rollback_point": null,
                            "head_matches": true,
                            "deleted_files": [],
                            "change_files": [],
                            "modified_files": [],
                        }));
                    }
                }
                Ok(serde_json::json!({
                    "rollback_point": m.superseded_point, // 灰显起点（选中的 X）
                    "head_matches": head_matches,
                    "deleted_files": m.deleted_files,
                    "change_files": m.change_files,
                    "modified_files": keinestamp::lore_project::rollback_modified_files(&meta.project_root)
                        .unwrap_or_default(),
                }))
            }
            Ok(None) => Ok(serde_json::json!({
                "rollback_point": null,
                "head_matches": true,
                "deleted_files": [],
                "change_files": [],
                "modified_files": [],
            })),
            Err(e) => Err(e.to_string()),
        }
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restore_rollback(path: String, discard_modified: Option<bool>) -> Result<(), String> {
    eprintln!(
        "[debug] restore_rollback({}, discard_modified={:?})",
        path, discard_modified
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        map_err(keinestamp::lore_project::undo_rollback(
            &meta.project_root,
            &hidden,
            discard_modified,
        ))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_rollback_conflicts(path: String) -> Result<Vec<String>, String> {
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    map_err(keinestamp::lore_project::rollback_conflicts(
        &meta.project_root,
    ))
}

#[tauri::command]
fn keep_rollback_file(path: String, file: String) -> Result<(), String> {
    eprintln!("[debug] keep_rollback_file({}, {})", path, file);
    let _ = (path, file);
    Ok(())
}

#[tauri::command]
async fn resolve_rollback_conflicts(path: String, choices: Vec<Vec<String>>) -> Result<(), String> {
    eprintln!(
        "[debug] resolve_rollback_conflicts({}, {} files)",
        path,
        choices.len()
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        // choices 每项 = [path, "current"|"stashed"]（current=当前版本改动，stashed=回退前改动）
        let pairs: Vec<(String, String)> = choices
            .into_iter()
            .filter_map(|c| match c.as_slice() {
                [path, choice] => Some((path.clone(), choice.clone())),
                _ => None,
            })
            .collect();
        map_err(keinestamp::lore_project::resolve_rollback_conflicts(
            &meta.project_root,
            &hidden,
            &pairs,
        ))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn status(path: String) -> Result<StatusResult, String> {
    // async + spawn_blocking：lore status 可达数秒，同步命令会阻塞主线程导致 UI 冻结
    let handle = tokio::task::spawn_blocking(move || -> Result<StatusResult, String> {
        let t0 = std::time::Instant::now();
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hidden = keinestamp::hidden::load_hidden(&meta.project_root);
        let files = map_err(keinestamp::lore_project::changed_files(
            &meta.project_root,
            &hidden,
        ))?;
        let elapsed = t0.elapsed();
        if elapsed.as_secs() >= 1 {
            eprintln!("[debug] status slow: {}ms", elapsed.as_millis());
        }
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
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn discard_file(path: String, file: String) -> Result<(), String> {
    // 取消单个文件更改（git restore 语义）：M 覆盖回 HEAD / D 从 HEAD 恢复。
    // 前端菜单仅对状态码 M/D 显示；staged/untracked 不在此范围
    eprintln!("[debug] discard_file({}, {})", path, file);
    validate_entry(&file)?;
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        map_err(keinestamp::lore_project::discard_file_changes(
            &meta.project_root,
            &file,
        ))
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn log(path: String, count: Option<usize>) -> Result<Vec<LogEntry>, String> {
    // spawn lore 可能耗时 → async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        eprintln!("[debug] log({}, count={:?})", path, count);
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let hist = map_err(keinestamp::lore_backend::revision_history(
            &meta.project_root,
        ))?;
        let c = count.unwrap_or(20);
        let entries: Vec<LogEntry> = hist
            .iter()
            .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
            .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("init"))
            .take(c)
            .map(|r| LogEntry {
                hash: r.id[..7.min(r.id.len())].to_string(),
                date: format_lore_date(&r.date),
                message: r.message.clone(),
            })
            .collect();
        eprintln!("[debug] log → {} entries (showing {})", entries.len(), c);
        Ok(entries)
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn read_config(path: String) -> Result<keinestamp::config::MetaConfig, String> {
    eprintln!("[debug] read_config({})", path);
    let p = PathBuf::from(&path);
    let meta_path = keinestamp::config::meta_path(&p);
    map_err(keinestamp::config::read_meta(&meta_path))
}

#[tauri::command]
fn update_config(path: String, key: String, value: Option<String>) -> Result<(), String> {
    eprintln!(
        "[debug] update_config({}, key={:?}, value={:?})",
        path, key, value
    );
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
        _ => {
            return Err(format!(
                "unknown config key: {key}. Available: access_token, site_name"
            ));
        }
    }

    map_err(keinestamp::config::write_meta(&meta_path, &meta))
}

#[tauri::command]
async fn export_entry(path: String, entry_id: String, output_dir: String) -> Result<(), String> {
    // dump + 逐文件 write 可能耗时 → async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        eprintln!(
            "[debug] export_entry({}, entry_id={}, output_dir={})",
            path, entry_id, output_dir
        );
        map_err(keinestamp::main_helpers::export_entry(
            Path::new(&path),
            &entry_id,
            Path::new(&output_dir),
        ))
    });
    handle.await.map_err(|e| e.to_string())?
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
            version: 1,
            project: String::new(),
            entries: vec![],
            rollback_marker: None,
            removed_entries: vec![],
        }),
    }
}

#[tauri::command]
fn get_tag_groups(path: String) -> Result<Vec<keinestamp::tag_groups::TagGroup>, String> {
    let p = PathBuf::from(&path);
    let meta = read_meta(&p)?;
    map_err(keinestamp::tag_groups::load_tag_groups(&meta.project_root))
}

#[tauri::command]
async fn compare_entries(
    path: String,
    entry_a: String,
    entry_b: String,
) -> Result<Vec<FileDiff>, String> {
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        let timeline = map_err(keinestamp::timeline::read_timeline(&timeline_path))?;

        let find = |id: &str| -> Result<keinestamp::timeline::TimelineEntry, String> {
            timeline
                .entries
                .iter()
                .find(|e| e.id == id || e.id.starts_with(id))
                .cloned()
                .ok_or_else(|| format!("entry not found: {}", id))
        };
        let a = find(&entry_a)?;
        let b = find(&entry_b)?;

        let mut files: Vec<String> = a.files.clone();
        for f in &b.files {
            if !files.contains(f) {
                files.push(f.clone());
            }
        }
        files.sort();
        files.dedup();

        let mut result = Vec::new();
        for file in files {
            let diff = keinestamp::lore_backend::run_lore(
                &meta.project_root,
                &["diff", "--source", &a.id, "--target", &b.id, &file],
            )
            .unwrap_or_default();
            result.push(FileDiff { file, diff });
        }
        Ok(result)
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_tracked_files(path: String) -> Result<Vec<String>, String> {
    // 全树递归 walk 可能耗时（大项目 + 每 10s 轮询）→ async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        let t0 = std::time::Instant::now();
        fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> std::io::Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                // v2：web/ 为部署产物目录（主仓库内），不计入文件列表
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "web"
                {
                    continue;
                }
                let rel = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", prefix, name)
                };
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
        let elapsed = t0.elapsed();
        if elapsed.as_secs() >= 1 {
            eprintln!("[debug] get_tracked_files slow: {}ms", elapsed.as_millis());
        }
        Ok(files)
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_empty_dirs(path: String) -> Result<Vec<String>, String> {
    // 空目录在文件树不显示（get_tracked_files 只列文件）→ 单独返回"无文件后代"的目录
    // （带尾斜杠，沿用 lore 目录行惯例 `out/`），前端并入 DirTree 渲染为文件夹节点
    let handle = tokio::task::spawn_blocking(move || {
        let t0 = std::time::Instant::now();
        // 返回该子树内是否有文件（含后代）；无 → 本目录为"空"目录，push 入 out
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<bool> {
            let mut has_files = false;
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "web"
                {
                    continue;
                }
                let rel = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", prefix, name)
                };
                if entry.file_type()?.is_dir() {
                    has_files |= walk(&entry.path(), &rel, out)?;
                } else {
                    has_files = true;
                }
            }
            if !has_files && !prefix.is_empty() {
                out.push(format!("{}/", prefix));
            }
            Ok(has_files)
        }
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let mut out = vec![];
        map_err(walk(&meta.project_root, "", &mut out).map_err(|e| anyhow::anyhow!("{}", e)))?;
        let elapsed = t0.elapsed();
        if elapsed.as_secs() >= 1 {
            eprintln!("[debug] get_empty_dirs slow: {}ms", elapsed.as_millis());
        }
        Ok(out)
    });
    handle.await.map_err(|e| e.to_string())?
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
async fn set_entry_milestone(
    path: String,
    entry_id: String,
    milestone: bool,
) -> Result<(), String> {
    eprintln!(
        "[debug] set_entry_milestone({}, entry_id={}, milestone={})",
        path, entry_id, milestone
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        // milestone 权威 = revision metadata，但 `lore revision metadata set` 只作用于
        // staged state（无 --revision 参数，实测报错），已提交 revision 无法回写 →
        // 方案 B：写 timeline.json 展示缓存的 milestone 字段（rebuild_cache 读取
        // metadata，下一记录后 milestone 恢复默认；Task 9 处理 metadata 方案）
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        map_err(keinestamp::timeline::set_entry_milestone(
            &timeline_path,
            &entry_id,
            milestone,
        ))?;

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

        // web 产物版本自增（.keinestamp/ 整体隔离，web 不入库）
        let _ = keinestamp::lore_project::bump_web_version(&meta.project_root);

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_entry_preview(path: String, entry_id: String, enabled: bool) -> Result<(), String> {
    eprintln!(
        "[debug] set_entry_preview({}, entry_id={}, enabled={})",
        path, entry_id, enabled
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        // 只写 timeline.json 展示缓存的开关字段（GLB 文件保留，重新开启即恢复）
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        map_err(keinestamp::timeline::set_entry_preview(
            &timeline_path,
            &entry_id,
            enabled,
        ))?;

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

        // 3D 预览开关属显示偏好：重建页面让本地生效，但不自增 web 版本号
        // （避免每次点击开关都触发"有新版本可部署"提示）
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_entry_preview_file(
    path: String,
    entry_id: String,
    preview_path: String,
    enabled: bool,
) -> Result<(), String> {
    eprintln!(
        "[debug] set_entry_preview_file({}, entry_id={}, preview_path={}, enabled={})",
        path, entry_id, preview_path, enabled
    );
    let handle = tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p)?;
        let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
        map_err(keinestamp::timeline::set_entry_preview_file(
            &timeline_path,
            &entry_id,
            &preview_path,
            enabled,
        ))?;

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

        // 单个预览开关同样属于显示偏好：重建页面但不自增 web 版本号
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

        map_err(keinestamp::timeline::remove_entry(
            &timeline_path,
            &entry_id,
        ))?;

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

        // web 产物版本自增（.keinestamp/ 整体隔离，web 不入库）
        let _ = keinestamp::lore_project::bump_web_version(&meta.project_root);

        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_project_log(path: String, count: Option<usize>) -> Result<Vec<String>, String> {
    // spawn lore 可能耗时 → async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        eprintln!("[debug] get_project_log({}, count={:?})", path, count);
        let p = PathBuf::from(&path);
        let c = count.unwrap_or(20);
        let meta = read_meta(&p)?;
        let hist = map_err(keinestamp::lore_backend::revision_history(
            &meta.project_root,
        ))?;
        Ok(hist
            .iter()
            .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
            .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("init"))
            .take(c)
            .map(|r| format!("{}  {}", &r.id[..7.min(r.id.len())], r.message))
            .collect())
    });
    handle.await.map_err(|e| e.to_string())?
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
async fn download_blender(
    app: tauri::AppHandle,
    version: Option<String>,
) -> Result<String, String> {
    eprintln!("[debug] download_blender(version={:?})", version);
    use tauri::Emitter;
    let version_clone = version.clone();
    let app_clone = app.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let mut progress_tx = |p: keinestamp::blender_download::DownloadProgress| {
            let _ = app_clone.emit(
                "blender-download-progress",
                BlenderProgress {
                    phase: p.phase.clone(),
                    message: p.message.clone(),
                    percent: p.percent,
                    bytes_downloaded: p.bytes_downloaded,
                    bytes_total: p.bytes_total,
                },
            );
        };
        keinestamp::blender_download::download_blender_with_progress(
            version_clone.as_deref(),
            &mut progress_tx,
        )
        .map(|p| p.to_string_lossy().to_string())
    });
    handle
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_blender_versions() -> Vec<String> {
    eprintln!("[debug] refresh_blender_versions: fetching from mirrors");
    let handle =
        tokio::task::spawn_blocking(|| keinestamp::blender_download::fetch_blender_versions());
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
async fn delete_blender_version(version: String) -> Result<(), String> {
    eprintln!("[debug] delete_blender_version({})", version);
    let handle = tokio::task::spawn_blocking(move || {
        keinestamp::blender_download::delete_blender_version(&version)
    });
    handle
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
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
fn get_current_blender_version() -> String {
    keinestamp::blender_download::current_blender_version()
}

#[tauri::command]
fn set_current_blender_version(version: String) -> Result<(), String> {
    keinestamp::blender_download::set_current_blender_version(&version).map_err(|e| e.to_string())
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
fn get_server_toggles() -> (bool, bool) {
    keinestamp::app_config::get_server_toggles()
}

#[tauri::command]
fn set_server_toggles(deploy_enabled: bool, lore_enabled: bool) {
    eprintln!(
        "[debug] set_server_toggles(deploy={}, lore={})",
        deploy_enabled, lore_enabled
    );
    keinestamp::app_config::set_server_toggles(deploy_enabled, lore_enabled);
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

#[tauri::command]
fn get_lore_server_url() -> String {
    keinestamp::app_config::get_lore_server_url()
}

#[tauri::command]
fn set_lore_server_url(url: String) {
    keinestamp::app_config::set_lore_server_url(&url);
}

#[tauri::command]
fn get_auto_restore_project() -> bool {
    keinestamp::app_config::get_auto_restore_project()
}

#[tauri::command]
fn set_auto_restore_project(enabled: bool) {
    keinestamp::app_config::set_auto_restore_project(enabled);
}

#[tauri::command]
async fn list_server_repos(url: String) -> Result<Vec<(String, String)>, String> {
    // 远程命令（连接可能超时）→ async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        keinestamp::lore_backend::list_server_repos(&url).map_err(|e| e.to_string())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_lore_identity() -> String {
    keinestamp::app_config::get_lore_identity()
}

#[tauri::command]
fn set_lore_identity(identity: String) {
    keinestamp::app_config::set_lore_identity(&identity);
}

#[tauri::command]
async fn check_lore_env() -> Result<serde_json::Value, String> {
    let handle = tokio::task::spawn_blocking(|| match keinestamp::lore_backend::check_lore_env() {
        Ok(info) => Ok(serde_json::json!({ "ok": true, "bin": info.bin, "version": info.version })),
        Err(e) => Err(e.to_string()),
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_server_repo(url: String, name: String) -> Result<(), String> {
    // 远程命令（连接可能超时）→ async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
        keinestamp::lore_backend::delete_server_repo(&url, &name).map_err(|e| e.to_string())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn test_lore_connection(host: String) -> bool {
    keinestamp::lore_backend::lore_server_health(&host)
}

/// 离线仓库迁移为在线（服务器按 ID 预注册 + push），迁移后可备份
#[tauri::command]
async fn migrate_repo(path: String) -> Result<(), String> {
    let handle = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let p = PathBuf::from(&path);
        let meta = read_meta(&p).map_err(|e| anyhow::anyhow!(e))?;
        keinestamp::lore_backend::migrate_offline_to_online(&meta.project_root)
    });
    handle
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn clone_server_repo(
    url: String,
    name: String,
    dest_parent: String,
) -> Result<String, String> {
    let handle = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let p = PathBuf::from(&dest_parent);
        // 克隆 + init 补洞（.keinestamp 结构 + loreignore + recent）
        let repo = keinestamp::lore_backend::clone_repo_to(&url, &name, &p)?;
        let meta_path = keinestamp::config::meta_path(&repo);
        if !meta_path.exists() {
            keinestamp::init::init_project(&repo)?;
        }
        keinestamp::recent::RecentProjects::add_path(&repo);
        Ok(repo.to_string_lossy().to_string())
    });
    handle
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

static LOCAL_SERVERS: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();

#[tauri::command]
async fn open_web_timeline(path: String) -> Result<(), String> {
    // recv_timeout(5s) 等服务端启动 → async 防主线程阻塞
    let handle = tokio::task::spawn_blocking(move || {
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
                        .map(|a| {
                            std::net::TcpStream::connect_timeout(
                                &a,
                                std::time::Duration::from_millis(300),
                            )
                            .is_ok()
                        })
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
                let p = rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .map_err(|_| "timeline server failed to start".to_string())?;
                guard.insert(key, p);
                p
            }
        };
        let url = format!("http://127.0.0.1:{}/", port);
        #[cfg(target_os = "windows")]
        let result = keinestamp::process_util::hide_console(std::process::Command::new("cmd"))
            .args(["/c", "start", &url])
            .spawn();
        #[cfg(target_os = "macos")]
        let result = keinestamp::process_util::hide_console(std::process::Command::new("open"))
            .arg(&url)
            .spawn();
        #[cfg(target_os = "linux")]
        let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open"))
            .arg(&url)
            .spawn();
        result.map_err(|e| format!("failed to open timeline: {}", e))?;
        Ok(())
    });
    handle.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_site(url: String) -> Result<(), String> {
    eprintln!("[debug] open_site({})", url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("invalid URL, must start with http:// or https://".to_string());
    }
    // 防御 shell 元字符注入（cmd /c start <url> 会解析 &| 等）
    if url.contains(['&', '|', '<', '>', '^', '"', '\'', '`', '\n', '\r']) {
        return Err("invalid URL: contains shell metacharacters".to_string());
    }
    #[cfg(target_os = "windows")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("cmd"))
        .args(["/c", "start", &url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("open"))
        .arg(&url)
        .spawn();
    #[cfg(target_os = "linux")]
    let result = keinestamp::process_util::hide_console(std::process::Command::new("xdg-open"))
        .arg(&url)
        .spawn();
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

            // 注入捆绑目录（安装包 resources/lore-bin；编译期注入的路径是构建机路径，运行时以资源目录为准）
            if let Ok(res_dir) = app.path().resource_dir() {
                keinestamp::lore_backend::set_bundled_lore_dir(res_dir.join("lore-bin"));
            }

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
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
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
            backup,
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
            get_tag_groups,
            compare_entries,
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
            set_entry_preview,
            set_entry_preview_file,
            get_project_log,
            get_tracked_files,
            get_empty_dirs,
            discard_file,
            download_blender,
            refresh_blender_versions,
            get_cached_blender_versions,
            list_installed_blender_versions,
            delete_blender_version,
            get_blender_path,
            set_blender_path,
            get_current_blender_version,
            set_current_blender_version,
            get_deploy_token,
            set_deploy_token,
            get_deploy_server_url,
            set_deploy_server_url,
            get_lore_server_url,
            set_lore_server_url,
            get_auto_restore_project,
            set_auto_restore_project,
            list_server_repos,
            clone_server_repo,
            delete_server_repo,
            test_lore_connection,
            migrate_repo,
            check_lore_env,
            get_lore_identity,
            set_lore_identity,
            get_auto_connect,
            set_auto_connect,
            get_server_toggles,
            set_server_toggles,
            get_close_action,
            set_close_action,
            resolve_close_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
