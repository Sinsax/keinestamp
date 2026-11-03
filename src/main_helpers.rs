use anyhow::Context;
use std::path::Path;

pub fn export_entry(project_root: &Path, entry_id: &str, output_dir: &Path) -> anyhow::Result<()> {
    let meta_path = crate::config::meta_path(project_root);
    let meta = crate::config::read_meta(&meta_path)?;

    let timeline_path = crate::timeline::timeline_cache_path(project_root);
    let timeline = crate::timeline::read_timeline(&timeline_path)
        .with_context(|| format!("failed to read timeline at {:?}", timeline_path))?;

    let entry = crate::timeline::get_entry(&timeline, entry_id)
        .ok_or_else(|| anyhow::anyhow!(crate::i18n::tf("cli.export.notFound", &[("entry", entry_id)])))?;

    if output_dir.exists() {
        anyhow::bail!(crate::i18n::tf("cli.export.dirExists", &[("path", &output_dir.display().to_string())]));
    }

    let files_dir = output_dir.join("files");
    crate::lore_project::export_entry(project_root, &entry.id, &files_dir)?;

    let preview_path = meta.web_root.join(&entry.preview);
    if entry.preview.is_empty() || !preview_path.exists() {
        eprintln!("{}", crate::i18n::t("cli.export.noThumbnail"));
    } else {
        // preview 可能是目录/尾斜杠路径 → file_name() 为 None，跳过复制而非 panic
        let Some(file_name) = preview_path.file_name() else {
            eprintln!("{}", crate::i18n::tf("cli.export.previewNoName", &[("path", &preview_path.display().to_string())]));
            return Ok(());
        };
        std::fs::copy(&preview_path, output_dir.join(file_name))
            .with_context(|| format!("failed to copy preview {:?}", preview_path))?;
    }

    let snapshot_json = serde_json::to_string_pretty(entry)?;
    std::fs::write(output_dir.join("snapshot.json"), &snapshot_json)
        .context("failed to write snapshot.json")?;

    let readme = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            crate::i18n::t("cli.export.readmeTitle"),
            crate::i18n::t("cli.export.readmeSep"),
            crate::i18n::tf("cli.export.readmeId", &[("value", &entry.id[..7.min(entry.id.len())])]),
            crate::i18n::tf(
                "cli.export.readmeTime",
                &[("value", &entry.timestamp[..19.min(entry.timestamp.len())].replace('T', " "))],
            ),
            crate::i18n::tf("cli.export.readmeAuthor", &[("value", &entry.author)]),
            crate::i18n::tf("cli.export.readmeMessage", &[("value", &entry.message)]),
            crate::i18n::tf("cli.export.readmeFiles", &[("value", &entry.files.len().to_string())]),
        );
    std::fs::write(output_dir.join("README.txt"), &readme).context("failed to write README.txt")?;

    println!("{}", crate::i18n::tf("cli.export.done", &[("id", &entry.id[..7.min(entry.id.len())]), ("path", &output_dir.display().to_string())]));
    Ok(())
}
