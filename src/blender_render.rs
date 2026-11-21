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
/// v4：修复 Blender 5.x 移除 `Image.use_alpha` 导致的导出崩溃（透明贴图改用 alpha_mode/depth 判定）。
/// v5：Draco 几何压缩（约 6×）+ 贴图限 2048/质量 85（体积再降），页面需 DRACOLoader。
const GLB_EXPORT_VERSION: u32 = 5;

/// Blender GLB 导出脚本：形态键移除 → Decimate 减面 → 透明贴图 BLEND → 导出。
/// 全部为内存操作（--background 不保存），.blend 原文件零改动。
/// 改动此脚本需同步递增 `GLB_EXPORT_VERSION`，否则旧 GLB 不会被重新导出。
const GLB_EXPORT_SCRIPT: &str = r#"import bpy
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

# 3) 贴图尺寸上限 2048px（预览足够；4K 原图会显著撑大体积，实测 2.35MB → 0.4MB）
MAX_TEX = 2048
for img in bpy.data.images:
    try:
        w, h = img.size
        if w > MAX_TEX or h > MAX_TEX:
            s = MAX_TEX / max(w, h)
            img.scale(max(1, int(w * s)), max(1, int(h * s)))
    except Exception:
        pass

# 4) 透明贴图：若材质使用了带 alpha 的图像，设为 BLEND 混合，保证 GLB 网页预览正确显示透明
for mat in bpy.data.materials:
    if mat is None or not mat.use_nodes or mat.node_tree is None:
        continue
    for node in mat.node_tree.nodes:
        if node.type != 'TEX_IMAGE' or node.image is None:
            continue
        # Blender 5.x 移除了 Image 的 use_alpha 属性：用 depth/alpha_mode 兼容判定
        has_alpha = getattr(node.image, 'use_alpha', None)
        if has_alpha is None:
            has_alpha = getattr(node.image, 'depth', 24) == 32 and getattr(node.image, 'alpha_mode', 'NONE') != 'NONE'
        if not has_alpha:
            continue
        try:
            mat.blend_method = 'BLEND'
        except Exception:
            pass
        break

bpy.ops.export_scene.gltf(
    filepath=output_path,
    export_format='GLB',
    export_image_format='WEBP',
    export_image_quality=85,
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
    # Draco 几何压缩：几何通常占 GLB 体积 90%+，实测 8.6MB → ~1MB
    export_draco_mesh_compression_enable=True,
    export_draco_mesh_compression_level=6,
    export_draco_position_quantization=14,
    export_draco_normal_quantization=10,
    export_draco_texcoord_quantization=12,
)

for name, state in saved.items():
    if name in bpy.data.objects:
        bpy.data.objects[name].hide_render = state
"#;

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


    let temp_script = output_dir.join(format!("glb_export_{}.py", std::process::id()));
    std::fs::write(&temp_script, GLB_EXPORT_SCRIPT)?;

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
            let detail = summarize_blender_error(&stderr);
            eprintln!(
                "[debug] export_to_glb: Blender exited 0 but GLB not found, stderr: {}",
                stderr.trim().chars().take(300).collect::<String>()
            );
            anyhow::bail!("Blender GLB export produced no output: {}", detail)
        }
        _ => {
            let stderr = String::from_utf8_lossy(&stderr);
            let detail = summarize_blender_error(&stderr);
            eprintln!(
                "[debug] export_to_glb: Blender failed: {}",
                stderr.trim().chars().take(200).collect::<String>()
            );
            anyhow::bail!("Blender GLB export failed: {}", detail)
        }
    }
}

/// 从 Blender stderr 里挑出最有诊断价值的一行，供 `preview_errors` 展示。
///
/// GUI 是 `windows_subsystem = "windows"`，没有控制台，`eprintln!` 里的真实错误
/// 用户永远看不到；只显示「未能生成预览」时无从排查（2026-11 芙姆米项目事故）。
/// 策略：优先最后一条 Python 异常行（`XxxError: ...`），否则取最后一个非空行；
/// 截断到 [`BLENDER_ERROR_MAX_CHARS`]，避免把整段 traceback 写进 timeline.json。
fn summarize_blender_error(stderr: &str) -> String {
    const BLENDER_ERROR_MAX_CHARS: usize = 300;
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let picked = lines
        .iter()
        .rev()
        .find(|l| {
            l.contains("Error:")
                || l.contains("Exception:")
                || l.contains("ERROR")
                || l.starts_with("error")
        })
        .or_else(|| lines.last())
        .copied()
        .unwrap_or("no stderr output");
    picked.chars().take(BLENDER_ERROR_MAX_CHARS).collect()
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
    fn test_summarize_blender_error_prefers_python_exception() {
        // 2026-11 事故的实际 stderr 形态：traceback + 末行版本信息
        let stderr = "Read blend: \"model.blend\"\n\
Traceback (most recent call last):\n\
  File \"glb_export.py\", line 55, in <module>\n\
    if node.type == 'TEX_IMAGE' and node.image and node.image.use_alpha:\n\
AttributeError: 'Image' object has no attribute 'use_alpha'\n\
Blender 5.2.0 LTS (hash fbe6228777e7 built 2026-07-14)\n\
Blender quit\n";
        assert_eq!(
            summarize_blender_error(stderr),
            "AttributeError: 'Image' object has no attribute 'use_alpha'"
        );
    }

    #[test]
    fn test_summarize_blender_error_falls_back_to_last_line() {
        assert_eq!(summarize_blender_error("\n\n  some detail  \n\n"), "some detail");
        assert_eq!(summarize_blender_error(""), "no stderr output");
    }

    #[test]
    fn test_summarize_blender_error_truncates() {
        let long = "E".repeat(1000);
        assert_eq!(summarize_blender_error(&long).chars().count(), 300);
    }

    /// 回归防护：Blender 5.x 移除了 `Image` 的 use_alpha 属性，直接访问会让导出脚本在
    /// `export_scene.gltf` 之前崩溃（且退出码仍为 0），预览全灭。
    #[test]
    fn test_export_script_does_not_use_removed_image_api() {
        assert!(
            !GLB_EXPORT_SCRIPT.contains(concat!(".use", "_alpha")),
            "导出脚本不得直接访问已移除的 Image 属性；请用 alpha_mode/depth 兼容判定"
        );
        assert!(
            GLB_EXPORT_SCRIPT.contains("alpha_mode"),
            "导出脚本应保留 alpha_mode 兼容判定（透明贴图 BLEND）"
        );
        assert!(GLB_EXPORT_SCRIPT.contains("export_scene.gltf"));
        assert!(GLB_EXPORT_SCRIPT.contains("import bpy"));
    }

    /// 回归防护：导出必须开启 Draco 几何压缩 + 贴图上限，否则预览文件回到十几 MB。
    #[test]
    fn test_export_script_enables_draco_and_texture_cap() {
        assert!(
            GLB_EXPORT_SCRIPT.contains("export_draco_mesh_compression_enable=True"),
            "必须开启 Draco 几何压缩（实测能把几何从 8.6MB 压到 ~1MB）"
        );
        assert!(GLB_EXPORT_SCRIPT.contains("MAX_TEX = 2048"));
        assert!(
            !GLB_EXPORT_SCRIPT.contains("export_image_quality=95"),
            "贴图质量应为 85（95 会让贴图占体积一半以上）"
        );
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
