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

/// GLB 导出参数版本：变更导出脚本（减面策略/纹理尺寸等）后 +1，
/// 旧版 GLB（无对应标记）会重新导出——否则"已存在跳过"会保留旧产物。
const GLB_EXPORT_VERSION: u32 = 3;

pub fn export_to_glb(
    blend_path: &Path,
    output_dir: &Path,
    output_name: &str,
    blender_path: Option<&Path>,
) -> Result<PathBuf> {
    eprintln!(
        "[debug] export_to_glb: blend={:?}, output_dir={:?}, name={}, blender={:?}",
        blend_path, output_dir, output_name, blender_path
    );
    std::fs::create_dir_all(output_dir)?;

    let output_path = output_dir.join(format!("{}.glb", output_name));
    let version_marker = output_dir.join(format!("{}.glb.v{}", output_name, GLB_EXPORT_VERSION));
    if output_path.exists() && version_marker.exists() {
        eprintln!(
            "[debug] export_to_glb: GLB already exists (v{}), skipping: {:?}",
            GLB_EXPORT_VERSION, output_path
        );
        return Ok(output_path);
    }
    if output_path.exists() {
        eprintln!(
            "[debug] export_to_glb: 旧版 GLB 无 v{} 标记，重新导出（瘦身参数变更）: {:?}",
            GLB_EXPORT_VERSION, output_path
        );
    }

    // 预览瘦身（v2）：临时移除形态键 → Decimate 减面（目标 ~5 万顶点）→ 贴图限 1024px。
    // 全部为内存操作（--background 不保存），.blend 原文件零改动。
    let python_script = r#"import bpy
import sys

output_path = sys.argv[sys.argv.index('--') + 1]

saved = {}
for obj in bpy.data.objects:
    saved[obj.name] = obj.hide_render
    if obj.hide_viewport or obj.hide_get():
        obj.hide_render = True

# 1) 移除形态键：Decimate(Collapse) 在带 shape keys 的网格上会异常/错乱；
#    形态键本来就不导出（export_morph=False），预览无需保留。
#    注意：不能直接删 reference_key（Basis）——Blender 可能拒绝，或把当前 active/last
#    形态键烘进网格。必须先删掉所有非 Basis 的 key block，再整体删除 shape_keys 数据块。
for obj in bpy.data.objects:
    if obj.type != 'MESH':
        continue
    key = obj.data.shape_keys
    if key is None:
        continue
    # 先清掉非 Basis 的形态键
    for kb in list(key.key_blocks):
        if kb != key.reference_key:
            try:
                obj.shape_key_remove(kb)
            except Exception:
                pass
    # 再删除整个 shape keys 数据块（含 Basis），确保网格回到 Basis 原始形态
    try:
        bpy.data.shape_keys.remove(key, do_unlink=True)
    except Exception:
        try:
            obj.shape_key_remove(key.reference_key)
        except Exception:
            pass

# 2) Decimate 减面（Collapse，目标 ~50000 顶点）：仅处理参与导出的可见网格
TARGET_VERTS = 50000
for obj in bpy.data.objects:
    if obj.type != 'MESH' or obj.hide_render or not obj.data.vertices:
        continue
    vcount = len(obj.data.vertices)
    if vcount > TARGET_VERTS:
        md = obj.modifiers.new(name='_ks_preview_decimate', type='DECIMATE')
        md.decimate_type = 'COLLAPSE'
        md.ratio = max(TARGET_VERTS / vcount, 0.05)

# 3) 贴图保持原始尺寸，不缩放（WebP 仍会编码，但分辨率不再降低）
# 4) 透明贴图：若材质使用了带 alpha 的图像，设为 BLEND 混合，保证 GLB 网页预览正确显示透明
for mat in bpy.data.materials:
    if mat is None or not mat.use_nodes or mat.node_tree is None:
        continue
    for node in mat.node_tree.nodes:
        if node.type == 'TEX_IMAGE' and node.image and node.image.use_alpha:
            mat.blend_method = 'BLEND'
            break

bpy.ops.export_scene.gltf(
    filepath=output_path,
    export_format='GLB',
    export_image_format='WEBP',
    export_image_quality=95,
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

    // 超时兜底 + 输出管道排空（blender 挂死/输出量大时防止 record 无限卡死）
    let label = format!("Blender GLB export: {}", blend_path.display());
    let result = crate::process_util::run_with_timeout(
        &mut blender_cmd(blender_path)
            .arg(blend_path)
            .arg("--python")
            .arg(&temp_script)
            .arg("--")
            .arg(&output_path)
            // 必须显式接管管道：run_with_timeout 依赖 take() 排空；不设则 stderr 捕获为空
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
        std::time::Duration::from_secs(300),
        &label,
    );
    let (status, _stdout, stderr) = match result {
        Ok(x) => x,
        Err(e) => {
            if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("[debug] export_to_glb: Blender not found in PATH");
                    anyhow::bail!(
                        "Blender not found in PATH. Please install Blender or configure the path."
                    )
                }
            }
            eprintln!("[debug] export_to_glb: failed to run Blender: {}", e);
            anyhow::bail!("Failed to run Blender: {}", e)
        }
    };

    eprintln!("[debug] export_to_glb: Blender process completed");

    let _ = std::fs::remove_file(&temp_script);

    match status {
        st if st.success() && output_path.exists() => {
            // 写导出版本标记（缓存失效依据：脚本参数变更后旧 GLB 重新导出）
            let _ = std::fs::write(&version_marker, "1");
            // 清理旧版本标记
            if let Ok(entries) = std::fs::read_dir(output_dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with(&format!("{}.glb.v", output_name))
                        && name
                            != version_marker
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                    {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            eprintln!("[debug] export_to_glb: success → {:?}", output_path);
            Ok(output_path)
        }
        st if st.success() => {
            let stderr = String::from_utf8_lossy(&stderr);
            eprintln!(
                "[debug] export_to_glb: Blender exited 0 but GLB not found, stderr: {}",
                stderr.trim().chars().take(300).collect::<String>()
            );
            anyhow::bail!(
                "Blender GLB export produced no output for {:?}: {}",
                blend_path,
                stderr.trim().chars().take(200).collect::<String>()
            )
        }
        _ => {
            let stderr = String::from_utf8_lossy(&stderr);
            eprintln!(
                "[debug] export_to_glb: Blender failed: {}",
                stderr.trim().chars().take(200).collect::<String>()
            );
            anyhow::bail!(
                "Blender GLB export failed for {:?}: {}",
                blend_path,
                stderr.trim()
            )
        }
    }
}

/// 清理 previews_glb 目录：**只删未被 timeline.json 引用的孤儿 GLB**（保留被引用的旧预览，
/// 否则记录后旧记录的 3D 标签因文件消失而失效）；超 max 时删最旧孤儿。
pub fn clean_old_glb_previews(project_root: &Path, dir: &Path, max: usize) -> Result<()> {
    eprintln!("[debug] clean_old_glb_previews: dir={:?}, max={}", dir, max);
    if !dir.exists() {
        return Ok(());
    }
    // timeline.json 引用的 preview 相对路径集合（previews_glb/xxx.glb）
    let referenced: std::collections::HashSet<String> =
        crate::timeline::read_timeline(&crate::timeline::timeline_cache_path(project_root))
            .map(|t| {
                let mut refs: std::collections::HashSet<String> = t
                    .entries
                    .iter()
                    .filter_map(|e| {
                        let p = e.preview.trim_start_matches("./");
                        (p.starts_with("previews_glb/") && p.ends_with(".glb"))
                            .then(|| p.to_string())
                    })
                    .collect();
                for e in &t.entries {
                    for p in &e.previews {
                        let rel = p.path.trim_start_matches("./");
                        if rel.starts_with("previews_glb/") && rel.ends_with(".glb") {
                            refs.insert(rel.to_string());
                        }
                    }
                }
                refs
            })
            .unwrap_or_else(|e| {
                eprintln!(
                    "[debug] clean_old_glb_previews: read timeline failed: {}",
                    e
                );
                std::collections::HashSet::new()
            });

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "glb"))
        .collect();
    // 只保留未被引用的孤儿
    entries.retain(|e| {
        let rel = format!("previews_glb/{}", e.file_name().to_string_lossy());
        !referenced.contains(&rel)
    });
    if entries.len() <= max {
        return Ok(());
    }
    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.created().or_else(|_| m.modified()))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    let to_remove = entries.len() - max;
    eprintln!(
        "[debug] clean_old_glb_previews: {} orphans, removing {}",
        entries.len(),
        to_remove
    );
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

    #[test]
    fn test_clean_old_glb_previews_protects_referenced() -> Result<()> {
        let root = std::env::temp_dir().join(format!("ks_glbclean_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("previews_glb")).unwrap();
        // 被引用 + 孤儿（2 个）：max=1 → 删最旧孤儿，引用文件保留
        std::fs::write(root.join("previews_glb/ref_old.glb"), "x").unwrap();
        std::fs::write(root.join("previews_glb/ref_new.glb"), "x").unwrap();
        std::fs::write(root.join("previews_glb/orphan1.glb"), "x").unwrap();
        std::fs::write(root.join("previews_glb/orphan2.glb"), "x").unwrap();
        // timeline.json 引用 ref_old/ref_new
        std::fs::create_dir_all(root.join(".keinestamp").join("web")).unwrap();
        let tl = serde_json::json!({
            "project": "t",
            "entries": [
                { "id": "a", "timestamp": "2026-08-01T00:00:00Z", "author": "lore", "message": "m1", "files": [], "preview": "previews_glb/ref_old.glb" },
                { "id": "b", "timestamp": "2026-08-02T00:00:00Z", "author": "lore", "message": "m2", "files": [], "preview": "previews_glb/ref_new.glb" }
            ],
            "rollback_marker": null
        });
        std::fs::write(
            crate::timeline::timeline_cache_path(&root),
            serde_json::to_string(&tl).unwrap(),
        )
        .unwrap();
        // max=1：只删最旧孤儿（引用文件即使超限也保留）
        clean_old_glb_previews(&root, &root.join("previews_glb"), 1).unwrap();
        assert!(
            root.join("previews_glb/ref_old.glb").exists(),
            "被引用文件应保留"
        );
        assert!(
            root.join("previews_glb/ref_new.glb").exists(),
            "被引用文件应保留"
        );
        let orphans_left: Vec<_> = std::fs::read_dir(root.join("previews_glb"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("orphan"))
            .collect();
        assert_eq!(orphans_left.len(), 1, "应删 1 个最旧孤儿，保留 1 个");
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
