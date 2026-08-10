use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub resolution: (u32, u32),
    pub camera_name: String,
    pub output_dir: PathBuf,
    pub max_previews: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            resolution: (1920, 1080),
            camera_name: "ProgressCamera".to_string(),
            output_dir: PathBuf::from("web/previews"),
            max_previews: 50,
        }
    }
}

fn blender_cmd(blender_path: Option<&Path>) -> std::process::Command {
    let exe = blender_path.unwrap_or_else(|| Path::new("blender"));
    eprintln!("[debug] blender_cmd: exe={:?}", exe);
    let mut cmd = crate::process_util::hide_console(std::process::Command::new(exe));
    cmd.arg("--background");
    cmd.arg("--factory-startup");
    cmd
}

pub fn export_to_glb(blend_path: &Path, output_dir: &Path, output_name: &str, blender_path: Option<&Path>) -> Result<PathBuf> {
    eprintln!("[debug] export_to_glb: blend={:?}, output_dir={:?}, name={}, blender={:?}", blend_path, output_dir, output_name, blender_path);
    std::fs::create_dir_all(output_dir)?;

    let output_path = output_dir.join(format!("{}.glb", output_name));
    if output_path.exists() {
        eprintln!("[debug] export_to_glb: GLB already exists, skipping: {:?}", output_path);
        return Ok(output_path);
    }

    let python_script = r#"import bpy
import sys

output_path = sys.argv[sys.argv.index('--') + 1]

saved = {}
for obj in bpy.data.objects:
    saved[obj.name] = obj.hide_render
    if obj.hide_viewport or obj.hide_get():
        obj.hide_render = True

bpy.ops.export_scene.gltf(
    filepath=output_path,
    export_format='GLB',
    export_image_format='WEBP',
    use_selection=False,
    use_visible=True,
    use_renderable=False,
    export_apply=True,
    export_texcoords=True,
    export_normals=True,
    export_materials='EXPORT',
    export_animations=False,
    export_skins=False,
    export_morph=False,
)

for name, state in saved.items():
    if name in bpy.data.objects:
        bpy.data.objects[name].hide_render = state
"#;

    let temp_script = output_dir.join(format!("glb_export_{}.py", std::process::id()));
    std::fs::write(&temp_script, python_script)?;

    let result = blender_cmd(blender_path)
        .arg(blend_path)
        .arg("--python")
        .arg(&temp_script)
        .arg("--")
        .arg(&output_path)
        .output();

    eprintln!("[debug] export_to_glb: Blender process completed");

    let _ = std::fs::remove_file(&temp_script);

    match result {
        Ok(o) if o.status.success() && output_path.exists() => {
            eprintln!("[debug] export_to_glb: success → {:?}", output_path);
            Ok(output_path)
        }
        Ok(o) if o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("[debug] export_to_glb: Blender exited 0 but GLB not found, stderr: {}", stderr.trim().chars().take(300).collect::<String>());
            anyhow::bail!("Blender GLB export produced no output for {:?}: {}", blend_path, stderr.trim().chars().take(200).collect::<String>())
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("[debug] export_to_glb: Blender failed: {}", stderr.trim().chars().take(200).collect::<String>());
            anyhow::bail!("Blender GLB export failed for {:?}: {}", blend_path, stderr.trim())
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("[debug] export_to_glb: Blender not found in PATH");
                anyhow::bail!("Blender not found in PATH. Please install Blender or configure the path.")
            }
            eprintln!("[debug] export_to_glb: failed to run Blender: {}", e);
            anyhow::bail!("Failed to run Blender: {}", e)
        }
    }
}

pub fn clean_old_glb_previews(dir: &Path, max: usize) -> Result<()> {
    eprintln!("[debug] clean_old_glb_previews: dir={:?}, max={}", dir, max);
    if !dir.exists() {
        eprintln!("[debug] clean_old_glb_previews: dir does not exist, skipping");
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "glb"))
        .collect();

    if entries.len() <= max {
        return Ok(());
    }

    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.created().or_else(|_| m.modified()))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    let to_remove = entries.len() - max;
    eprintln!("[debug] clean_old_glb_previews: {} entries, removing {}", entries.len(), to_remove);
    for entry in entries.iter().take(to_remove) {
        std::fs::remove_file(entry.path())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(config.resolution, (1920, 1080));
        assert_eq!(config.camera_name, "ProgressCamera");
        assert_eq!(config.max_previews, 50);
    }
}
