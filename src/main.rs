use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "keinestamp", version, about = "Keinestamp - Blender Git Progress Board")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(name = "record", alias = "commit")]
    Record {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long)]
        message: String,

        #[arg(short, long)]
        tag: Option<String>,

        #[arg(long, help = "Skip 3D preview generation")]
        no_preview: bool,
    },

    Push {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Pull {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Revert {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Restore {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Log {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short = 'n', long, default_value = "20")]
        count: usize,
    },

    Config {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long, value_name = "KEY=VALUE")]
        set: Option<String>,
    },

    Export {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long)]
        entry: String,

        #[arg(short, long)]
        output: PathBuf,
    },

    Serve {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long, default_value = "7799", help = "Port to listen on")]
        port: u16,

        #[arg(long, default_value = "0.0.0.0", help = "Host to bind to")]
        host: String,

        #[arg(long, help = "Path to serve.toml config file")]
        config: Option<PathBuf>,
    },

    Deploy {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long, required = true, help = "Server URL (e.g. http://192.168.0.100:7799)")]
        server: String,

        #[arg(long, help = "Deploy token (Bearer auth)")]
        deploy_token: Option<String>,
    },

    DownloadBlender {
        #[arg(short, long)]
        version: Option<String>,
    },

    ListBlenderVersions,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init { path }) => {
            let result = keinestamp::init::init_project(&path);
            if result.is_ok() {
                keinestamp::recent::RecentProjects::add_path(&path);
            }
            result
        }
        Some(Commands::Record { path, message, tag, no_preview }) => {
            let result = cmd_record(&path, &message, tag.as_deref(), no_preview);
            if result.is_ok() {
                keinestamp::recent::RecentProjects::add_path(&path);
            }
            result
        }
        Some(Commands::Push { path }) => cmd_push(&path),
        Some(Commands::Pull { path }) => cmd_pull(&path),
        Some(Commands::Revert { path }) => cmd_revert(&path),
        Some(Commands::Restore { path }) => cmd_restore(&path),
        Some(Commands::Status { path }) => cmd_status(&path),
        Some(Commands::Log { path, count }) => cmd_log(&path, count),
        Some(Commands::Config { path, set }) => cmd_config(&path, set.as_deref()),
        Some(Commands::Export { path, entry, output }) => cmd_export(&path, &entry, &output),
        Some(Commands::Serve { path, port, host, config }) => cmd_serve(&path, port, &host, config.as_deref()),
        Some(Commands::Deploy { path, server, deploy_token }) => cmd_deploy(&path, &server, deploy_token.as_deref()),

        Some(Commands::DownloadBlender { version }) => {
            cmd_download_blender(version.as_deref())
        }
        Some(Commands::ListBlenderVersions) => {
            let versions = keinestamp::blender_download::fetch_blender_versions()
                .unwrap_or_else(|_| keinestamp::blender_download::list_blender_versions());
            println!("可用 Blender 版本：");
            for (i, v) in versions.iter().enumerate() {
                let marker = if i == 0 { " (最新)" } else { "" };
                println!("  {}{}", v, marker);
            }
            Ok(())
        }
        None => {
            println!("Keinestamp - Blender Git Progress Board");
            println!("用法: keinestamp <子命令> [选项]");
            println!("子命令: init, record, push, pull, revert, status, log, config, export, serve, deploy, download-blender, list-blender-versions");
            println!("GUI 模式: 运行 `cargo tauri dev` 或使用编译后的 Tauri 应用");
            println!("查看详细帮助: keinestamp --help");
            Ok(())
        }
    }
}

fn load_meta(path: &Path) -> anyhow::Result<keinestamp::config::MetaConfig> {
    let meta_path = keinestamp::config::meta_path(path);
    keinestamp::config::read_meta(&meta_path)
}

fn cmd_record(path: &Path, message: &str, tag: Option<&str>, no_preview: bool) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    if let Some(lr) = keinestamp::app_config::get_last_revert() {
        if meta.project_id.as_deref() == Some(lr.project_id.as_str()) {
            if let Ok(head) = keinestamp::git_project::get_current_commit_hash(&meta.project_root) {
                if head == lr.revert_hash {
                    eprintln!("⚠️ 注意：当前处于撤回状态，记录后将无法恢复上次撤回的记录（其内容会合并进新记录）");
                }
            }
        }
    }

    keinestamp::git_project::ensure_git_user(&meta.project_root)?;
    keinestamp::git_project::lfs_auto_configure(&meta.project_root)?;

    let hash = keinestamp::git_project::add_and_commit(&meta.project_root, message)?;

    let short_hash = &hash[..7.min(hash.len())];
    let timestamp = chrono::Utc::now().to_rfc3339();

    let changed_files = keinestamp::git_helpers::get_changed_files(&meta.project_root, &hash)?;
    let changed_files_status = keinestamp::git_helpers::get_changed_files_status(&meta.project_root, &hash)?;
    let timeline_path = meta.web_root.join("timeline.json");

    let blender_path_buf = if !no_preview && keinestamp::git_helpers::has_blend_files(
        &keinestamp::git_helpers::get_changed_files(&meta.project_root, &hash)?,
    ) {
        match keinestamp::blender_download::ensure_blender() {
            Ok(path) => Some(path),
            Err(e) => {
                eprintln!("Blender 自动下载失败: {}，尝试使用 PATH 中的 blender", e);
                keinestamp::app_config::get_blender_path()
            }
        }
    } else {
        keinestamp::app_config::get_blender_path()
    };
    let blender_path: Option<&Path> = blender_path_buf.as_deref();

    let preview_path = if !no_preview && keinestamp::git_helpers::has_blend_files(&changed_files) {
        let glb_config = keinestamp::blender_render::RenderConfig {
            output_dir: meta.web_root.join("previews_glb"),
            ..Default::default()
        };
        for bf in &changed_files {
            if bf.ends_with(".blend") {
                let safe_name = bf.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                let bpath = meta.project_root.join(bf);
                match keinestamp::blender_render::export_to_glb(&bpath, &glb_config.output_dir, &glb_name, blender_path) {
                    Ok(_) => (),
                    Err(e) => eprintln!("GLB 导出跳过: {}", e),
                }
            }
        }
        keinestamp::blender_render::clean_old_glb_previews(
            &meta.web_root.join("previews_glb"),
            glb_config.max_previews,
        )?;
        Some(String::new())
    } else {
        None
    };

    let entry = keinestamp::timeline::TimelineEntry {
        id: hash.clone(),
        timestamp,
        author: keinestamp::git_helpers::get_git_author(&meta.project_root),
        message: message.to_string(),
        files: changed_files.clone(),
        file_status: changed_files_status.clone(),
        preview: preview_path.unwrap_or_default(),
        tag: tag.map(|s| s.to_string()),
        milestone: false,
    };

    keinestamp::timeline::append_entry(&timeline_path, entry)?;

    let project_name = meta.project_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let mut timeline = keinestamp::timeline::read_timeline(&timeline_path)?;
    timeline.project = project_name.clone();
    keinestamp::timeline::write_timeline(&timeline_path, &timeline)?;

    keinestamp::html_gen::generate_index_html(
        &meta.web_root,
        &meta.project_root,
        &timeline,
        &project_name,
    )?;

    let _ = keinestamp::git_web::web_commit_and_push(
        &meta.web_root,
        &format!("update timeline: {}", message),
        false,
    );

    println!("已记录 {} — {}", short_hash, message);
    Ok(())
}

fn cmd_push(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    if keinestamp::git_project::has_remote(&meta.project_root) {
        keinestamp::git_project::push(&meta.project_root)?;
        println!("已推送到远端");
    } else {
        eprintln!("[debug] cmd_push: no git remote, skipping git push");
    }

    Ok(())
}

fn cmd_pull(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    if keinestamp::git_project::has_remote(&meta.project_root) {
        keinestamp::git_project::pull(&meta.project_root)?;
        println!("已从远端拉取");
    } else {
        println!("没有配置 git 远端，跳过拉取");
    }

    Ok(())
}

fn cmd_revert(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let original_hash = keinestamp::git_project::get_current_commit_hash(&meta.project_root)?;

    match keinestamp::git_project::revert(&meta.project_root) {
        Ok(_) => {
            let revert_hash = keinestamp::git_project::get_current_commit_hash(&meta.project_root)?;
            let timeline_path = meta.web_root.join("timeline.json");
            let mut removed_entry: Option<keinestamp::timeline::TimelineEntry> = None;
            if let Ok(mut tl) = keinestamp::timeline::read_timeline(&timeline_path) {
                if let Some(e) = tl.entries.pop() {
                    eprintln!("[debug] revert: removed entry {}", &e.id[..7.min(e.id.len())]);
                    removed_entry = Some(e.clone());
                    keinestamp::timeline::write_timeline(&timeline_path, &tl)?;
                    let project_name = meta.project_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    keinestamp::html_gen::generate_index_html(&meta.web_root, &meta.project_root, &tl, &project_name)?;
                    let _ = keinestamp::git_web::web_commit_and_push(
                        &meta.web_root,
                        &format!("revert {}", &e.id[..7.min(e.id.len())]),
                        false,
                    );
                }
            }
            if let Some(pid) = meta.project_id.clone() {
                keinestamp::app_config::set_last_revert(Some(keinestamp::app_config::LastRevert {
                    project_id: pid,
                    revert_hash,
                    original_hash,
                    removed_entry,
                }));
            }
            println!("已撤回上一次提交，文件修改保留在工作区中");
            Ok(())
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("no commits to revert") {
                println!("没有可撤回的提交");
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn cmd_restore(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let lr = keinestamp::app_config::get_last_revert();
    let lr = match lr {
        Some(r) if meta.project_id.as_deref() == Some(r.project_id.as_str()) => r,
        _ => {
            println!("没有可恢复的撤回");
            return Ok(());
        }
    };
    let head = keinestamp::git_project::get_current_commit_hash(&meta.project_root)?;
    if head != lr.revert_hash {
        println!("撤回后已有新记录，无法恢复");
        return Ok(());
    }
    keinestamp::git_project::restore_revert(&meta.project_root, &lr.original_hash)?;
    keinestamp::app_config::set_last_revert(None);
    if let Some(entry) = lr.removed_entry {
        keinestamp::timeline::append_entry(&meta.web_root.join("timeline.json"), entry)?;
    }
    if let Ok(tl) = keinestamp::timeline::read_timeline(&meta.web_root.join("timeline.json")) {
        let project_name = meta.project_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        keinestamp::html_gen::generate_index_html(&meta.web_root, &meta.project_root, &tl, &project_name)?;
        let _ = keinestamp::git_web::web_commit_and_push(
            &meta.web_root,
            &format!("restore revert {}", &lr.original_hash[..7.min(lr.original_hash.len())]),
            false,
        );
    }
    println!("已恢复撤回的上一次记录");
    Ok(())
}

fn cmd_status(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    let output = keinestamp::git_project::status(&meta.project_root)?;
    if output.trim().is_empty() {
        println!("工作区干净，没有未提交的修改");
    } else {
        println!("未提交的修改：");
        for line in output.lines() {
            println!("  {}", line);
        }
    }

    Ok(())
}

fn cmd_log(path: &Path, count: usize) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = match keinestamp::timeline::read_timeline(&timeline_path) {
        Ok(t) => t,
        Err(_) => {
            println!("暂无提交记录");
            return Ok(());
        }
    };

    if timeline.entries.is_empty() {
        println!("暂无提交记录");
        return Ok(());
    }

    let project = if timeline.project.is_empty() {
        "Keinestamp".to_string()
    } else {
        timeline.project.clone()
    };

    println!("{} 提交历史（最近 {} 条）：", project, count);

    let entries: Vec<_> = timeline.entries.iter().rev().take(count).collect();
    for entry in &entries {
        let short_hash = &entry.id[..7.min(entry.id.len())];
        let date = entry.timestamp[..19.min(entry.timestamp.len())]
            .replace('T', " ");
        let file_count = entry.files.len();
        println!(
            "  {}  {}  {}  ({} 文件)",
            short_hash, date, entry.author, file_count
        );
        println!("         {}", entry.message);
    }

    Ok(())
}

fn cmd_export(path: &Path, entry_id: &str, output_dir: &Path) -> anyhow::Result<()> {
    keinestamp::main_helpers::export_entry(path, entry_id, output_dir)
}

fn cmd_serve(path: &Path, port: u16, host: &str, config_path: Option<&Path>) -> anyhow::Result<()> {
    let config = if let Some(cp) = config_path {
        keinestamp::serve::load_config(cp)?
    } else {
        let serve_toml = path.join("serve.toml");
        if serve_toml.exists() {
            keinestamp::serve::load_config(&serve_toml)?
        } else {
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| path.to_path_buf());
            let web_root = exe_dir.join("web-root");
            keinestamp::serve::auto_generate_config(&serve_toml, port, host, &web_root)?
        }
    };
    keinestamp::serve::start(config)
}

fn cmd_deploy(path: &Path, server: &str, deploy_token: Option<&str>) -> anyhow::Result<()> {
    let mut meta = load_meta(path)?;
    let app_token = keinestamp::app_config::get_deploy_token();
    let token = deploy_token.or(app_token.as_deref());
    let app_server = keinestamp::app_config::get_deploy_server_url();
    let server_url = if server.is_empty() || server == "." {
        app_server.as_deref().unwrap_or("")
    } else {
        server
    };
    if server_url.is_empty() {
        anyhow::bail!("未设置服务器地址，请通过 --server 参数或软件设置中配置");
    }
    if meta.project_id.is_none() {
        meta.project_id = Some(keinestamp::config::generate_project_id());
        keinestamp::config::write_meta(&keinestamp::config::meta_path(path), &meta)?;
    }
    let project_id = meta.project_id.as_deref().unwrap_or("");
    let project_name = keinestamp::config::resolve_site_name(&meta);
    let prefix = keinestamp::push::deploy_to_server(&meta.web_root, server_url, token, meta.access_token.as_deref(), &project_name, project_id)?;
    let url = if prefix.is_empty() {
        server_url.to_string()
    } else {
        format!("{}{}", server_url.trim_end_matches('/'), prefix)
    };
    meta.site_name = Some(project_name);
    meta.site_locked = true;
    keinestamp::config::write_meta(&keinestamp::config::meta_path(path), &meta)?;
    println!("已部署到 {}", url);
    Ok(())
}

fn cmd_download_blender(version: Option<&str>) -> anyhow::Result<()> {
    match keinestamp::blender_download::download_blender(version) {
        Ok(path) => {
            println!("Blender 已下载到: {}", path.display());
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn cmd_config(path: &Path, set: Option<&str>) -> anyhow::Result<()> {
    if let Some(kv) = set {
        return cmd_config_set(path, kv);
    }
    cmd_config_show(path)
}

fn cmd_config_show(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    println!("当前配置：");
    println!("  版本:           {}", meta.version);
    println!("  初始化时间:     {}", meta.init_time);
    println!("  工程根目录:     {}", meta.project_root.display());
    println!("  网页输出目录:   {}", meta.web_root.display());

    println!(
        "  自动连接:       {}",
        if keinestamp::app_config::get_auto_connect() { "是" } else { "否" }
    );

    if let Some(ref token) = meta.access_token {
        println!("  访问令牌:       {}****", &token[..4.min(token.len())]);
    } else {
        println!("  访问令牌:       (未设置)");
    }

    Ok(())
}

fn cmd_config_set(path: &Path, kv: &str) -> anyhow::Result<()> {
    let equal_pos = kv.find('=').ok_or_else(|| {
        anyhow::anyhow!("格式错误，请使用 key=value 格式，例如 access_token=abc123")
    })?;

    let key = &kv[..equal_pos];
    let value_str = &kv[equal_pos + 1..];
    let value = if value_str.is_empty() { None } else { Some(value_str) };

    let meta_path = keinestamp::config::meta_path(path);
    let mut meta = keinestamp::config::read_meta(&meta_path)?;

    match key {
        "access_token" => {
            meta.access_token = value.map(|s| s.to_string());
        }
        "auto_connect" => {
            let v = value.unwrap_or("false");
            keinestamp::app_config::set_auto_connect(v == "true" || v == "1" || v == "yes");
        }
        _ => {
            anyhow::bail!(
                "未知配置项: {}。可用项: access_token, auto_connect（软件级）",
                key
            );
        }
    }

    keinestamp::config::write_meta(&meta_path, &meta)?;
    println!("已更新 {} = {}", key, value.unwrap_or("(清空)"));

    Ok(())
}