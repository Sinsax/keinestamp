use anyhow::Context;
use std::path::Path;

pub fn export_entry(project_root: &Path, entry_id: &str, output_dir: &Path) -> anyhow::Result<()> {
    let meta_path = crate::config::meta_path(project_root);
    let meta = crate::config::read_meta(&meta_path)?;

    let timeline_path = crate::timeline::timeline_cache_path(project_root);
    let timeline = crate::timeline::read_timeline(&timeline_path)
        .with_context(|| format!("failed to read timeline at {:?}", timeline_path))?;

    let entry = crate::timeline::get_entry(&timeline, entry_id)
        .ok_or_else(|| anyhow::anyhow!("未找到 entry: {}", entry_id))?;

    if output_dir.exists() {
        anyhow::bail!("输出目录已存在: {:?}", output_dir);
    }

    let files_dir = output_dir.join("files");
    crate::lore_project::export_entry(project_root, &entry.id, &files_dir)?;

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
        &entry.id[..7.min(entry.id.len())],
        &entry.timestamp[..19.min(entry.timestamp.len())].replace('T', " "),
        entry.author,
        entry.message,
        entry.files.len(),
    );
    std::fs::write(output_dir.join("README.txt"), &readme)
        .context("failed to write README.txt")?;

    println!("已导出提交 {} 到 {:?}", &entry.id[..7.min(entry.id.len())], output_dir);
    Ok(())
}
