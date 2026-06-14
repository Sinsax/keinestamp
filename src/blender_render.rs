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
            output_dir: PathBuf::from("web-timeline/previews"),
            max_previews: 50,
        }
    }
}

fn blender_cmd(blender_path: Option<&Path>) -> std::process::Command {
    let exe = blender_path.unwrap_or_else(|| Path::new("blender"));
    eprintln!("[debug] blender_cmd: exe={:?}", exe);
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--background");
    cmd.arg("--factory-startup");
    cmd
}

pub fn render_preview(blend_path: &Path, config: &RenderConfig, output_name: &str, blender_path: Option<&Path>) -> Result<PathBuf> {
    std::fs::create_dir_all(&config.output_dir)?;

    let output_path = config.output_dir.join(format!("{}.png", output_name));

    let python_script = generate_render_script(&config);

    let temp_script = config
        .output_dir
        .join(format!("render_progress_{}.py", std::process::id()));
    std::fs::write(&temp_script, &python_script)?;

    let output = blender_cmd(blender_path)
        .arg(blend_path)
        .arg("--python")
        .arg(&temp_script)
        .arg("--")
        .arg(&output_path)
        .output();

    let _ = std::fs::remove_file(&temp_script);

    match output {
        Ok(o) if o.status.success() => {
            Ok(output_path)
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            anyhow::bail!(
                "Blender render failed for {:?}: {}",
                blend_path,
                stderr.trim()
            );
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::bail!(
                    "Blender not found in PATH. Please install Blender or configure the path."
                );
            }
            anyhow::bail!("Failed to run Blender: {}", e);
        }
    }
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

pub fn clean_old_previews(dir: &Path, max: usize) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
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
    for entry in entries.iter().take(to_remove) {
        std::fs::remove_file(entry.path())?;
    }

    Ok(())
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

fn generate_render_script(config: &RenderConfig) -> String {
    format!(
        r#"import bpy
import sys

output_path = sys.argv[sys.argv.index('--') + 1]

bpy.context.scene.render.resolution_x = {width}
bpy.context.scene.render.resolution_y = {height}
bpy.context.scene.render.image_settings.file_format = 'PNG'

camera = bpy.data.objects.get('{camera}')
if camera:
    bpy.context.scene.camera = camera
elif not bpy.context.scene.camera:
    for obj in bpy.data.objects:
        if obj.type == 'CAMERA':
            bpy.context.scene.camera = obj
            break

if not bpy.context.scene.camera:
    sys.exit(0)

bpy.context.scene.render.filepath = output_path
bpy.ops.render.render(write_still=True)
"#,
        width = config.resolution.0,
        height = config.resolution.1,
        camera = config.camera_name,
    )
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

    #[test]
    fn test_generate_render_script() {
        let config = RenderConfig::default();
        let script = generate_render_script(&config);
        assert!(script.contains("1920"));
        assert!(script.contains("1080"));
        assert!(script.contains("ProgressCamera"));
        assert!(script.contains("bpy.ops.render.render(write_still=True)"));
    }

    #[test]
    fn test_clean_old_previews_empty_dir() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_clean_empty");
        std::fs::create_dir_all(&dir)?;
        clean_old_previews(&dir, 5)?;
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_clean_old_previews_under_limit() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_clean_under");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("a.png"), b"")?;
        std::fs::write(dir.join("b.png"), b"")?;
        clean_old_previews(&dir, 10)?;
        assert!(dir.join("a.png").exists());
        assert!(dir.join("b.png").exists());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_clean_old_previews_over_limit() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_clean_over");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("old.png"), b"")?;
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(dir.join("new.png"), b"")?;
        clean_old_previews(&dir, 1)?;
        assert!(!dir.join("old.png").exists());
        assert!(dir.join("new.png").exists());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
