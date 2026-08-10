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
    },

    Push {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Pull {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Rollback {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long, help = "回退到的记录 ID（timeline entry id，支持前缀）")]
        entry: String,
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
        Some(Commands::Record { path, message }) => {
            let result = cmd_record(&path, &message);
            if result.is_ok() {
                keinestamp::recent::RecentProjects::add_path(&path);
            }
            result
        }
        Some(Commands::Push { path }) => cmd_push(&path),
        Some(Commands::Pull { path }) => cmd_pull(&path),
        Some(Commands::Rollback { path, entry }) => cmd_rollback(&path, &entry),
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
            println!("子命令: init, record, push, pull, rollback, restore, status, log, config, export, serve, deploy, download-blender, list-blender-versions");
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

fn cmd_record(path: &Path, message: &str) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let hidden = keinestamp::hidden::load_hidden(&meta.project_root);

    if let Ok(rs) = keinestamp::lore_project::rollback_state(&meta.project_root) {
        if rs.active {
            eprintln!("⚠️ 注意：当前处于回退状态，记录后将顶替被回退的记录（原记录被新记录取代）");
        }
    }

    let rev = keinestamp::lore_project::record(&meta.project_root, message, None, None, &hidden)?;
    // 回退状态下记录 = 顶替，成功后清除回退标记
    let _ = keinestamp::lore_project::clear_rollback(&meta.project_root);

    let short_hash = &rev[..7.min(rev.len())];
    println!("已提交 {} — {}", short_hash, message);
    Ok(())
}

fn cmd_push(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    keinestamp::lore_backend::run_lore(&meta.project_root, &["push"])
        .map_err(|e| anyhow::anyhow!("推送失败: {}", e))?;
    println!("已推送到远端");
    Ok(())
}

fn cmd_pull(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    keinestamp::lore_backend::run_lore(&meta.project_root, &["sync"])
        .map_err(|e| anyhow::anyhow!("同步失败: {}", e))?;
    println!("已从远端拉取");
    Ok(())
}

fn cmd_rollback(path: &Path, entry: &str) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let hidden = keinestamp::hidden::load_hidden(&meta.project_root);

    let timeline_path = keinestamp::timeline::timeline_cache_path(&meta.project_root);
    let tl = keinestamp::timeline::read_timeline(&timeline_path)?;
    let full = keinestamp::timeline::get_entry(&tl, entry)
        .map(|e| e.id.clone())
        .ok_or_else(|| anyhow::anyhow!("记录不存在: {}", entry))?;

    // 回退状态下再次回退：先撤销上次回退，再回退到新目标（恢复目标始终为首次回退前）
    if keinestamp::lore_project::rollback_state(&meta.project_root)?.active {
        keinestamp::lore_project::undo_rollback(&meta.project_root, &hidden)
            .map_err(|e| anyhow::anyhow!("先撤销上次回退失败: {}", e))?;
    }

    keinestamp::lore_project::rollback(&meta.project_root, &full, &hidden)?;
    println!("已回退到记录 {}（可用 restore 恢复）", &full[..7.min(full.len())]);
    Ok(())
}

fn cmd_restore(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let hidden = keinestamp::hidden::load_hidden(&meta.project_root);

    let st = keinestamp::lore_project::rollback_state(&meta.project_root)?;
    if !st.active {
        anyhow::bail!("没有可恢复的回退");
    }
    keinestamp::lore_project::undo_rollback(&meta.project_root, &hidden)?;
    println!("已恢复回退前的状态");
    Ok(())
}

fn cmd_status(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;
    let hidden = keinestamp::hidden::load_hidden(&meta.project_root);

    let files = keinestamp::lore_project::changed_files(&meta.project_root, &hidden)?;
    if files.is_empty() {
        println!("工作区干净，没有未提交的修改");
    } else {
        println!("未提交的修改：");
        for f in &files {
            println!("  {} {}", state_code(&f.state), f.path);
        }
    }

    Ok(())
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

fn cmd_log(path: &Path, count: usize) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    let hist = keinestamp::lore_backend::revision_history(&meta.project_root)?;
    let entries: Vec<_> = hist
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) != Some("web"))
        .collect();

    if entries.is_empty() {
        println!("暂无提交记录");
        return Ok(());
    }

    let project = meta
        .project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Keinestamp".to_string());

    println!("{} 提交历史（最近 {} 条）：", project, count);

    // 文件数从 timeline 缓存取（展示缓存，rebuild_cache 在每次 record 时重建）；拿不到则显示 0
    let mut file_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    if let Ok(tl) = keinestamp::timeline::read_timeline(&keinestamp::timeline::timeline_cache_path(&meta.project_root)) {
        for e in &tl.entries {
            file_counts.insert(e.id.clone(), e.files.len());
        }
    }

    for r in entries.iter().take(count) {
        let short_id = &r.id[..7.min(r.id.len())];
        let date = format_date(&r.date);
        let file_count = file_counts.get(&r.id).copied().unwrap_or(0);
        println!(
            "  {}  {}  {}  ({} 文件)",
            short_id, date, "lore", file_count
        );
        println!("         {}", r.message);
    }

    Ok(())
}

fn format_date(s: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(s)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| s.to_string())
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
