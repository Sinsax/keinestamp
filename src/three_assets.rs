use anyhow::{Context, Result};
use std::path::Path;

const THREE_CJS: &[u8] = include_bytes!("three-src/three.cjs");

pub(crate) const ORBIT_CONTROLS_SRC: &[u8] = include_bytes!("three-src/controls/OrbitControls.js");

pub(crate) const GLTF_LOADER_SRC: &[u8] = include_bytes!("three-src/loaders/GLTFLoader.js");

const BUFFER_GEOMETRY_UTILS_SRC: &[u8] = include_bytes!("three-src/utils/BufferGeometryUtils.js");

pub fn ensure_threejs(three_dir: &Path) -> Result<()> {
    let old_files = ["three.module.js"];
    for old in &old_files {
        let path = three_dir.join(old);
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    let subdirs = ["controls", "loaders", "utils"];
    for subdir in &subdirs {
        let dir = three_dir.join(subdir);
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {:?}/{}", three_dir, subdir))?;
        }
    }

    let files: [(&str, &[u8], bool); 4] = [
        ("three.cjs", THREE_CJS, false),
        ("controls/OrbitControls.js", ORBIT_CONTROLS_SRC, true),
        ("loaders/GLTFLoader.js", GLTF_LOADER_SRC, true),
        ("utils/BufferGeometryUtils.js", BUFFER_GEOMETRY_UTILS_SRC, true),
    ];

    for (name, data, needs_transform) in &files {
        let dest = three_dir.join(name);

        let content = std::str::from_utf8(data)
            .with_context(|| format!("three.js file {} is not valid UTF-8", name))?;

        let expected = if *needs_transform {
            transform_addon_js(content)
        } else {
            content.to_string()
        };

        if dest.exists() {
            if let Ok(existing) = std::fs::read_to_string(&dest) {
                if existing == expected {
                    continue;
                }
            }
        }

        eprintln!("[debug] ensure_threejs: writing {}", name);
        std::fs::write(&dest, &expected)
            .with_context(|| format!("failed to write {:?}", dest))?;
    }

    Ok(())
}

pub(crate) fn transform_addon_js(content: &str) -> String {
    let mut result = String::new();
    result.push_str("(function(){\nvar THREE = window.THREE;\n");

    let mut in_import_block = false;
    let mut in_export_block = false;
    let mut export_names: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if in_export_block {
            if trimmed.starts_with('}') && trimmed.ends_with(';') {
                in_export_block = false;
            } else if trimmed.starts_with('}') && !trimmed.ends_with(',') {
                in_export_block = false;
            } else {
                for name in trimmed.split(',') {
                    let name = name.trim().trim_end_matches(',').trim();
                    if name.is_empty() { continue; }
                    export_names.push(name.to_string());
                }
            }
            continue;
        }

        // Multi-line export block start
        if trimmed.starts_with("export {") && !trimmed.ends_with("};") && !trimmed.ends_with("} from") {
            in_export_block = true;
            let rest = trimmed.trim_start_matches("export {").trim();
            if !rest.is_empty() && rest != "}" {
                for name in rest.split(',') {
                    let name = name.trim().trim_end_matches(',').trim();
                    if name.is_empty() { continue; }
                    export_names.push(name.to_string());
                }
            }
            continue;
        }

        // Single-line export: export { Foo };
        if trimmed.starts_with("export {") && trimmed.ends_with("};") {
            let names_part = trimmed.trim_start_matches("export {")
                .trim_end_matches("};")
                .trim();
            if !names_part.is_empty() {
                for name in names_part.split(',') {
                    let name = name.trim();
                    if !name.is_empty() { export_names.push(name.to_string()); }
                }
            }
            continue;
        }

        if trimmed == "export {" || trimmed == "export {" {
            in_export_block = true;
            continue;
        }

        if trimmed.starts_with("export default") {
            continue;
        }

        // Strip "export " from class/function/const/let/var declarations
        if trimmed.starts_with("export ") {
            let after = &trimmed[7..];
            if after.starts_with("class ") || after.starts_with("function ")
                || after.starts_with("const ") || after.starts_with("let ")
                || after.starts_with("var ")
            {
                let kw_end = after.find(' ').unwrap_or(after.len());
                let rest = after[kw_end..].trim();
                let first_word_end = rest.find(|c: char| c == ' ' || c == '(' || c == '{' || c == '<' || c == '\n').unwrap_or(rest.len());
                let name = &rest[..first_word_end];
                if !name.is_empty() && name != "{" && name != "(" {
                    export_names.push(name.to_string());
                }
            }
            result.push_str(&format!("  {}\n", after));
            continue;
        }

        // Import blocks
        if in_import_block {
            if trimmed.ends_with("} from 'three';") || trimmed.ends_with("} from \"three\";") {
                in_import_block = false;
            } else if trimmed.starts_with('}') && !trimmed.contains("from") {
                if trimmed == "}" { in_import_block = false; }
            } else if trimmed.ends_with("} from '../utils/BufferGeometryUtils.js';") {
                in_import_block = false;
            }
            continue;
        }

        if trimmed.starts_with("import {") && trimmed.contains("} from 'three';") {
            continue;
        }
        if trimmed.starts_with("import ") && trimmed.contains(" from '../utils/") {
            let content = trimmed.trim_start_matches("import ");
            let brace_part = content.split("} from").next().unwrap_or(content);
            let names_clean = brace_part.trim_start_matches('{').trim_end_matches('}').trim();
            for name in names_clean.split(',') {
                let name = name.trim();
                if name.is_empty() { continue; }
                let parts: Vec<&str> = name.splitn(2, " as ").map(|s| s.trim()).collect();
                let alias = parts.get(1).unwrap_or(&parts[0]);
                result.push_str(&format!("  var {} = window.{};\n", alias, parts[0]));
            }
            continue;
        }
        if trimmed.starts_with("import {") {
            in_import_block = true;
            continue;
        }

        // Skip remaining import/export lines
        if trimmed.starts_with("import ") { continue; }

        result.push_str("  ");
        result.push_str(line);
        result.push('\n');
    }

    for name in &export_names {
        let n = name.trim().trim_end_matches(',');
        if !n.is_empty() {
            result.push_str(&format!("window.{} = {};\n", n, n));
        }
    }
    result.push_str("})();\n");
    result
}
