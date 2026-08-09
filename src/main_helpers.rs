use anyhow::Context;
use std::path::Path;

pub fn export_entry(project_root: &Path, entry_id: &str, output_dir: &Path) -> anyhow::Result<()> {
    let meta_path = crate::config::meta_path(project_root);
    let meta = crate::config::read_meta(&meta_path)?;

    let timeline_path = meta.web_root.join("timeline.json");
    let timeline = crate::timeline::read_timeline(&timeline_path)
        .with_context(|| format!("failed to read timeline at {:?}", timeline_path))?;

    let entry = crate::timeline::get_entry(&timeline, entry_id)
        .ok_or_else(|| anyhow::anyhow!("未找到 entry: {}", entry_id))?;

    if output_dir.exists() {
        anyhow::bail!("输出目录已存在: {:?}", output_dir);
    }

    let files_dir = output_dir.join("files");
    std::fs::create_dir_all(&files_dir)
        .with_context(|| format!("failed to create output directory {:?}", files_dir))?;

    let ls_output = crate::process_util::hide_console(std::process::Command::new("git"))
        .args(["-c", "core.quotepath=false", "ls-tree", "-r", "--name-only", &entry.id])
        .current_dir(project_root)
        .output()
        .context("failed to list files in commit")?;

    if !ls_output.status.success() {
        let stderr = String::from_utf8_lossy(&ls_output.stderr);
        anyhow::bail!("git ls-tree 失败: {}", stderr.trim());
    }

    let files: Vec<String> = String::from_utf8_lossy(&ls_output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if files.is_empty() {
        anyhow::bail!("提交 {} 中没有文件", &entry.id[..7]);
    }

    for file in &files {
        let show_output = crate::process_util::hide_console(std::process::Command::new("git"))
            .args(["show", &format!("{}:{}", entry.id, file)])
            .current_dir(project_root)
            .output()
            .with_context(|| format!("failed to extract file: {}", file))?;

        if !show_output.status.success() {
            let stderr = String::from_utf8_lossy(&show_output.stderr);
            anyhow::bail!("git show {} 失败: {}", file, stderr.trim());
        }

        let dest = files_dir.join(file);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {:?}", parent))?;
        }
        std::fs::write(&dest, &show_output.stdout)
            .with_context(|| format!("failed to write {:?}", dest))?;
    }

    let preview_path = meta.web_root.join(&entry.preview);
    if entry.preview.is_empty() || !preview_path.exists() {
        eprintln!("(该提交无缩略图)");
    } else {
        std::fs::copy(&preview_path, output_dir.join(preview_path.file_name().unwrap()))
            .with_context(|| format!("failed to copy preview {:?}", preview_path))?;
    }

    let snapshot_json = serde_json::to_string_pretty(entry)?;
    std::fs::write(output_dir.join("snapshot.json"), &snapshot_json)
        .context("failed to write snapshot.json")?;

    let readme = format!(
        r#"提交导出快照
==============
提交:     {}
时间:     {}
作者:     {}
消息:     {}
文件数:   {}
"#,
        &entry.id[..7],
        &entry.timestamp[..19].replace('T', " "),
        entry.author,
        entry.message,
        entry.files.len(),
    );
    std::fs::write(output_dir.join("README.txt"), &readme)
        .context("failed to write README.txt")?;

    println!("已导出提交 {} 到 {:?}", &entry.id[..7], output_dir);
    Ok(())
}
