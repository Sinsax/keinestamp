use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "saki-step", version, about = "SakiStep - Blender Git Progress Board")]
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

    Commit {
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
            let result = saki_step::init::init_project(&path);
            if result.is_ok() {
                saki_step::recent::RecentProjects::add_path(&path);
            }
            result
        }
        Some(Commands::Commit { path, message, tag, no_preview }) => {
            let result = cmd_commit(&path, &message, tag.as_deref(), no_preview);
            if result.is_ok() {
                saki_step::recent::RecentProjects::add_path(&path);
            }
            result
        }
        Some(Commands::Push { path }) => cmd_push(&path),
        Some(Commands::Pull { path }) => cmd_pull(&path),
        Some(Commands::Revert { path }) => cmd_revert(&path),
        Some(Commands::Status { path }) => cmd_status(&path),
        Some(Commands::Log { path, count }) => cmd_log(&path, count),
        Some(Commands::Config { path, set }) => cmd_config(&path, set.as_deref()),
        Some(Commands::Export { path, entry, output }) => cmd_export(&path, &entry, &output),
        Some(Commands::DownloadBlender { version }) => {
            cmd_download_blender(version.as_deref())
        }
        Some(Commands::ListBlenderVersions) => {
            let versions = saki_step::blender_download::fetch_blender_versions()
                .unwrap_or_else(|_| saki_step::blender_download::list_blender_versions());
            println!("可用 Blender 版本：");
            for (i, v) in versions.iter().enumerate() {
                let marker = if i == 0 { " (最新)" } else { "" };
                println!("  {}{}", v, marker);
            }
            Ok(())
        }
        None => {
            println!("SakiStep - Blender Git Progress Board");
            println!("用法: saki-step <子命令> [选项]");
            println!("子命令: init, commit, push, pull, revert, status, log, config, export, download-blender, list-blender-versions");
            println!("GUI 模式: 运行 `cargo tauri dev` 或使用编译后的 Tauri 应用");
            println!("查看详细帮助: saki-step --help");
            Ok(())
        }
    }
}

fn load_meta(path: &Path) -> anyhow::Result<saki_step::config::MetaConfig> {
    let meta_path = saki_step::config::meta_path(path);
    saki_step::config::read_meta(&meta_path)
}

fn cmd_commit(path: &Path, message: &str, tag: Option<&str>, no_preview: bool) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    saki_step::git_project::ensure_git_user(&meta.project_root)?;
    saki_step::git_project::lfs_auto_configure(&meta.project_root)?;

    let hash = saki_step::git_project::add_and_commit(&meta.project_root, message)?;

    let short_hash = &hash[..7.min(hash.len())];
    let timestamp = chrono::Utc::now().to_rfc3339();

    let changed_files = saki_step::git_helpers::get_changed_files(&meta.project_root, &hash)?;
    let changed_files_status = saki_step::git_helpers::get_changed_files_status(&meta.project_root, &hash)?;
    let timeline_path = meta.web_root.join("timeline.json");

    let blender_path_buf = if !no_preview && saki_step::git_helpers::has_blend_files(
        &saki_step::git_helpers::get_changed_files(&meta.project_root, &hash)?,
    ) {
        match saki_step::blender_download::ensure_blender() {
            Ok(path) => Some(path),
            Err(e) => {
                eprintln!("Blender 自动下载失败: {}，尝试使用 PATH 中的 blender", e);
                saki_step::app_config::get_blender_path()
            }
        }
    } else {
        saki_step::app_config::get_blender_path()
    };
    let blender_path: Option<&Path> = blender_path_buf.as_deref();

    let preview_path = if !no_preview && saki_step::git_helpers::has_blend_files(&changed_files) {
        let glb_config = saki_step::blender_render::RenderConfig {
            output_dir: meta.web_root.join("previews_glb"),
            ..Default::default()
        };
        for bf in &changed_files {
            if bf.ends_with(".blend") {
                let safe_name = bf.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                let bpath = meta.project_root.join(bf);
                match saki_step::blender_render::export_to_glb(&bpath, &glb_config.output_dir, &glb_name, blender_path) {
                    Ok(_) => (),
                    Err(e) => eprintln!("GLB 导出跳过: {}", e),
                }
            }
        }
        saki_step::blender_render::clean_old_glb_previews(
            &meta.web_root.join("previews_glb"),
            glb_config.max_previews,
        )?;
        Some(String::new())
    } else {
        None
    };

    let entry = saki_step::timeline::TimelineEntry {
        id: hash.clone(),
        timestamp,
        author: saki_step::git_helpers::get_git_author(&meta.project_root),
        message: message.to_string(),
        files: changed_files.clone(),
        file_status: changed_files_status.clone(),
        preview: preview_path.unwrap_or_default(),
        tag: tag.map(|s| s.to_string()),
    };

    saki_step::timeline::append_entry(&timeline_path, entry)?;

    let project_name = meta.project_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let mut timeline = saki_step::timeline::read_timeline(&timeline_path)?;
    timeline.project = project_name.clone();
    saki_step::timeline::write_timeline(&timeline_path, &timeline)?;

    saki_step::html_gen::generate_index_html(
        &meta.web_root,
        &meta.project_root,
        &timeline,
        &project_name,
    )?;

    let _ = saki_step::git_web::web_commit_and_push(
        &meta.web_root,
        &format!("update timeline: {}", message),
        meta.web_push_enabled,
    );

    println!("已提交 {} — {}", short_hash, message);
    Ok(())
}

fn cmd_push(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    saki_step::git_project::push(&meta.project_root)?;
    println!("已推送到远端");

    if meta.web_push_enabled {
        let _ = saki_step::git_web::web_commit_and_push(
            &meta.web_root,
            "push",
            true,
        );
    }

    Ok(())
}

fn cmd_pull(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    saki_step::git_project::pull(&meta.project_root)?;
    println!("已从远端拉取");

    Ok(())
}

fn cmd_revert(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    match saki_step::git_project::revert(&meta.project_root) {
        Ok(_) => {
            let timeline_path = meta.web_root.join("timeline.json");
            if let Ok(mut tl) = saki_step::timeline::read_timeline(&timeline_path) {
                if let Some(removed) = tl.entries.pop() {
                    saki_step::timeline::write_timeline(&timeline_path, &tl)?;
                    let project_name = meta.project_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    saki_step::html_gen::generate_index_html(&meta.web_root, &meta.project_root, &tl, &project_name)?;
                    let _ = saki_step::git_web::web_commit_and_push(
                        &meta.web_root,
                        &format!("revert {}", &removed.id[..7.min(removed.id.len())]),
                        meta.web_push_enabled,
                    );
                }
            }
            println!("已撤回上一次提交，文件修改保留在工作区中");
            Ok(())
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("HEAD~1") {
                println!("没有可撤回的提交");
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn cmd_status(path: &Path) -> anyhow::Result<()> {
    let meta = load_meta(path)?;

    let output = saki_step::git_project::status(&meta.project_root)?;
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
    let timeline = match saki_step::timeline::read_timeline(&timeline_path) {
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
        "SakiStep".to_string()
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
    saki_step::main_helpers::export_entry(path, entry_id, output_dir)
}

fn cmd_download_blender(version: Option<&str>) -> anyhow::Result<()> {
    match saki_step::blender_download::download_blender(version) {
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

    if let Some(ref remote) = meta.web_remote {
        println!("  网页远端:       {}", remote);
    } else {
        println!("  网页远端:       (未设置)");
    }

    println!("  自动推送网页:   {}", if meta.web_push_enabled { "是" } else { "否" });

    Ok(())
}

fn cmd_config_set(path: &Path, kv: &str) -> anyhow::Result<()> {
    let equal_pos = kv.find('=').ok_or_else(|| {
        anyhow::anyhow!("格式错误，请使用 key=value 格式，例如 web_remote=https://example.com")
    })?;

    let key = &kv[..equal_pos];
    let value_str = &kv[equal_pos + 1..];
    let value = if value_str.is_empty() { None } else { Some(value_str) };

    let meta_path = saki_step::config::meta_path(path);
    let mut meta = saki_step::config::read_meta(&meta_path)?;

    match key {
        "web_remote" => {
            meta.web_remote = value.map(|s| s.to_string());
            if meta.web_remote.is_some() && !meta.web_push_enabled {
                meta.web_push_enabled = true;
            }
        }
        "web_push_enabled" => {
            let v = value.unwrap_or("false");
            meta.web_push_enabled = v == "true" || v == "1" || v == "yes";
        }
        _ => {
            anyhow::bail!(
                "未知配置项: {}。可用项: web_remote, web_push_enabled",
                key
            );
        }
    }

    saki_step::config::write_meta(&meta_path, &meta)?;
    println!("已更新 {} = {}", key, value.unwrap_or("(清空)"));

    Ok(())
}