use crate::timeline::Timeline;
use anyhow::{Context, Result};
use std::path::Path;

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn compute_diff(project_root: &Path, entry_id: &str, file_path: &str) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c", "core.quotepath=false",
            "show",
            entry_id,
            "--",
            file_path,
        ])
        .current_dir(project_root)
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

pub fn generate_index_html(
    web_root: &Path,
    project_root: &Path,
    timeline: &Timeline,
    page_title: &str,
) -> Result<()> {
    eprintln!("[debug] generate_index_html: web_root={:?}, {} entries", web_root, timeline.entries.len());
    let timeline_json = serde_json::to_string_pretty(timeline)?;

    let mut diffs_map = std::collections::BTreeMap::new();
    for entry in &timeline.entries {
        let mut entry_diffs = std::collections::BTreeMap::new();
        for file in &entry.files {
            if file.ends_with(".blend") { continue; }
            let diff = compute_diff(project_root, &entry.id, file);
            if !diff.is_empty() {
                entry_diffs.insert(file.clone(), diff);
            }
        }
        if !entry_diffs.is_empty() {
            diffs_map.insert(entry.id.clone(), entry_diffs);
        }
    }
    let diffs_json = serde_json::to_string(&diffs_map)?;

    let mut glb_map = std::collections::BTreeMap::new();
    for entry in &timeline.entries {
        for file in &entry.files {
            if file.ends_with(".blend") {
                let short_hash = &entry.id[..7.min(entry.id.len())];
                let safe_name = file.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                let glb_name = format!("{}_{}", short_hash, safe_name.replace(".blend", ""));
                let glb_path = web_root.join("previews_glb").join(format!("{}.glb", glb_name));
                if glb_path.exists() {
                    let b64 = std::fs::read(&glb_path)
                        .map(|b| base64_encode(&b))
                        .unwrap_or_default();
                    glb_map.entry(entry.id.clone())
                        .or_insert_with(Vec::new)
                        .push(serde_json::json!({"file": file, "base64": b64}));
                }
            }
        }
    }
    let glb_json = serde_json::to_string(&glb_map)?;

    let mut calendar: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for entry in &timeline.entries {
        if let Some(date) = entry.timestamp.split('T').next() {
            if !date.is_empty() {
                calendar.entry(date.to_string()).or_default().push(entry.id.clone());
            }
        }
    }
    let calendar_json = serde_json::to_string(&calendar)?;

    let three_dir = web_root.join("three");
    ensure_threejs(&three_dir)?;

    let html = TEMPLATE
        .replace("{{TIMELINE_JSON}}", &timeline_json)
        .replace("{{GLB_JSON}}", &glb_json)
        .replace("{{DIFFS_JSON}}", &diffs_json)
        .replace("{{CALENDAR_JSON}}", &calendar_json)
        .replace("{{PAGE_TITLE}}", page_title);

    std::fs::write(web_root.join("index.html"), &html)
        .with_context(|| format!("failed to write index.html to {:?}", web_root))?;

    Ok(())
}

const THREE_CJS: &[u8] = include_bytes!("three-src/three.cjs");

const ORBIT_CONTROLS_SRC: &[u8] = include_bytes!("three-src/controls/OrbitControls.js");

const GLTF_LOADER_SRC: &[u8] = include_bytes!("three-src/loaders/GLTFLoader.js");

const BUFFER_GEOMETRY_UTILS_SRC: &[u8] = include_bytes!("three-src/utils/BufferGeometryUtils.js");

fn ensure_threejs(three_dir: &Path) -> Result<()> {
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

fn transform_addon_js(content: &str) -> String {
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

const TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{{PAGE_TITLE}} - SakiStep</title>
<script>var module = {exports: {}}; var exports = module.exports;</script>
<script src="./three/three.cjs"></script>
<script>window.THREE = module.exports;</script>
<script src="./three/utils/BufferGeometryUtils.js"></script>
<script src="./three/controls/OrbitControls.js"></script>
<script src="./three/loaders/GLTFLoader.js"></script>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #1e1e2e; color: #cdd6f4; line-height: 1.6; }
.page { display: flex; min-height: 100vh; }
.left-col { width: 230px; flex-shrink: 0; background: #181825; border-right: 1px solid #313244; display: flex; flex-direction: column; padding: 16px 12px; position: sticky; top: 0; height: 100vh; overflow-y: auto; }
.right-col { flex: 1; min-width: 0; max-width: 800px; margin: 0 auto; padding: 40px 24px; }
h1 { font-size: 28px; font-weight: 700; margin-bottom: 8px; color: #f5f5f7; }
.subtitle { color: #6c7086; margin-bottom: 32px; }
.entry { background: #181825; border-radius: 12px; padding: 20px; margin-bottom: 16px; border: 1px solid #313244; border-left: 3px solid #313244; }
.entry.tagged { border-left-color: var(--tag-color, #89b4fa); }
.entry-header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
.entry-id { font-size: 12px; color: #6c7086; font-family: monospace; }
.entry-date { font-size: 14px; color: #6c7086; }
.entry-message { font-size: 16px; font-weight: 500; margin-bottom: 8px; color: #f5f5f7; }
.tag-badge { display: inline-block; font-size: 11px; font-weight: 600; padding: 1px 8px; border-radius: 10px; margin-left: 8px; vertical-align: middle; color: #1e1e2e; }
.entry-files { margin-top: 6px; }
.empty { text-align: center; color: #6c7086; padding: 60px 0; font-size: 16px; }
.tag-group { display: flex; align-items: center; gap: 8px; margin: 20px 0 8px; }
.tag-group-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
.tag-group-label { font-size: 13px; font-weight: 700; color: #cdd6f4; }
.tag-group-line { flex: 1; height: 1px; background: #313244; }

.sec-hdr { display: flex; align-items: center; gap: 5px; margin-top: 8px; padding: 5px 0; font-size: 12px; font-weight: 600; color: #6c7086; cursor: pointer; user-select: none; border-top: 1px solid #313244; }
.sec-hdr:hover { color: #cdd6f4; }
.sec-hdr .arr { font-size: 9px; width: 10px; flex-shrink: 0; }
.sec-body { padding: 4px 0 2px 16px; font-size: 13px; }
.fs-line { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-family: monospace; font-size: 12px; }
.fs-badge { width: 18px; text-align: center; font-weight: 700; font-size: 13px; flex-shrink: 0; }
.fs-path { color: #cdd6f4; }
.st-add { color: #4caf88; }
.st-del { color: #e06c75; }
.st-mod { color: #e5c07b; }
.diff-entry { margin-bottom: 6px; }
.diff-path { font-size: 11px; font-family: monospace; color: #89b4fa; padding: 2px 6px; background: rgba(0,0,0,0.25); border-radius: 3px; margin-bottom: 3px; }
.diff-empty { font-size: 11px; color: #6c7086; padding: 2px 6px; }
.diff-loading { font-size: 11px; color: #6c7086; padding: 4px 6px; }
.dl { display: flex; gap: 8px; font-family: monospace; font-size: 11px; line-height: 1.6; padding: 0 4px; border-radius: 2px; }
.dl-num { width: 32px; text-align: right; flex-shrink: 0; color: #6c7086; font-size: 10px; user-select: none; }
.dl-text { white-space: pre; flex: 1; }
.dl.add { background: rgba(76, 175, 136, 0.12); color: #4caf88; }
.dl.add .dl-num { color: #4caf88; }
.dl.del { background: rgba(224, 108, 117, 0.12); color: #e06c75; }
.dl.del .dl-num { color: #e06c75; }
.dl.mod { background: rgba(100, 140, 255, 0.12); color: #648cff; }
.dl.mod .dl-num { color: #648cff; }
.cal { }
.cal-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
.cal-title { font-size: 13px; font-weight: 700; color: #cdd6f4; }
.cal-nav { display: flex; gap: 4px; }
.cal-nav button { width: 20px; height: 20px; background: rgba(69,71,90,0.4); border: 1px solid #45475a; border-radius: 3px; color: #a6adc8; cursor: pointer; font-size: 10px; line-height: 1; display: flex; align-items: center; justify-content: center; }
.cal-nav button:hover { background: rgba(69,71,90,0.8); }
.cal-month { display: flex; align-items: center; gap: 4px; margin-bottom: 2px; }
.cal-mlabel { font-size: 9px; color: #585b70; width: 14px; flex-shrink: 0; text-align: right; }
.cal-days { display: flex; gap: 2px; flex-wrap: wrap; }
.cal-cday { width: 6px; height: 6px; border-radius: 1px; background: #313244; cursor: pointer; }
.cal-cday:hover { outline: 1px solid #89b4fa; outline-offset: 0; }
.cal-cday.h1 { background: #1a6d3c; }
.cal-cday.h2 { background: #238a4a; }
.cal-cday.h3 { background: #30a758; }
.cal-cday.h4 { background: #40c468; }
.cal-cday.today { outline: 1px solid #f9e2af; outline-offset: 0; }
.cal-legend { display: flex; align-items: center; gap: 2px; margin-top: 6px; font-size: 8px; color: #585b70; }
.cal-legend .s { width: 8px; height: 8px; border-radius: 1px; background: #313244; display: inline-block; }
.cal-legend .s.s1 { background: #1a6d3c; }
.cal-legend .s.s2 { background: #238a4a; }
.cal-legend .s.s3 { background: #30a758; }
.cal-legend .s.s4 { background: #40c468; }
.left-footer { margin-top: auto; display: flex; flex-direction: column; gap: 4px; padding-top: 12px; border-top: 1px solid #313244; }
.left-footer-btn { display: flex; align-items: center; justify-content: center; gap: 6px; padding: 8px; border-radius: 6px; background: rgba(69,71,90,0.3); border: 1px solid #45475a; color: #cdd6f4; cursor: pointer; font-size: 12px; font-weight: 600; transition: background 0.12s; }
.left-footer-btn:hover { background: rgba(69,71,90,0.6); }
.left-footer-btn:disabled { opacity: 0.3; cursor: default; pointer-events: none; }
.model-placeholder { font-size: 11px; color: #6c7086; padding: 8px; text-align: center; background: #181825; border-radius: 6px; }
.model-box { width: 100%; border-radius: 6px; overflow: hidden; background: #1a1a2e; position: relative; }
.model-toolbar { position: absolute; top: 0; left: 0; right: 0; display: flex; align-items: center; gap: 4px; padding: 6px 8px; background: rgba(17,17,27,0.82); z-index: 10; font-size: 11px; }
.model-toolbar .sep { width: 1px; height: 16px; background: #45475a; margin: 0 4px; }
.mbtn { padding: 3px 8px; background: rgba(69,71,90,0.5); border: 1px solid #45475a; border-radius: 4px; color: #a6adc8; cursor: pointer; font-size: 10px; font-family: inherit; transition: background 0.15s, color 0.15s; white-space: nowrap; }
.mbtn:hover { background: rgba(69,71,90,0.9); }
.mbtn.active { background: rgba(137,180,250,0.2); border-color: #89b4fa; color: #89b4fa; }
.model-expand-btn { width: 26px; height: 26px; background: rgba(30,30,46,0.85); border: 1px solid #45475a; border-radius: 4px; color: #cdd6f4; font-size: 14px; cursor: pointer; display: flex; align-items: center; justify-content: center; line-height: 1; transition: background 0.2s; margin-left: auto; }
.model-expand-btn:hover { background: rgba(69,71,90,0.95); }
.model-overlay { position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; z-index: 9999; background: #11111b; display: flex; flex-direction: column; }
.model-overlay-bar { display: flex; align-items: center; gap: 4px; padding: 6px 12px; background: #181825; border-bottom: 1px solid #313244; flex-shrink: 0; font-size: 11px; }
.model-overlay-bar .sep { width: 1px; height: 16px; background: #45475a; margin: 0 4px; }
.model-overlay-close { width: 26px; height: 26px; background: rgba(224,108,117,0.15); border: 1px solid #e06c75; border-radius: 4px; color: #e06c75; font-size: 16px; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: background 0.2s; margin-left: auto; }
.model-overlay-close:hover { background: rgba(224,108,117,0.3); }
.model-overlay-canvas { flex: 1; position: relative; }

</style>
</head>
<body>
<div class="page">
  <div class="left-col">
    <div id="cal-sidebar"></div>
    <div class="left-footer">
      <span style="font-size:10px;color:#6c7086;text-align:center">SakiStep</span>
    </div>
  </div>
  <div class="right-col">
    <div id="app"></div>
  </div>
</div>
<script>
var data = {{TIMELINE_JSON}};
var glbData = {{GLB_JSON}};
var diffsData = {{DIFFS_JSON}};
var calData = {{CALENDAR_JSON}};

var TAG_PRESETS = {'建模':0,'UV拆分':1,'绑定':2,'贴图':3,'修复':4};
var TAG_COLORS = ['#89b4fa','#a6e3a1','#cba6f7','#fab387','#f38ba8','#94e2d5'];

function tagColor(tag) {
  if (TAG_PRESETS[tag] !== undefined) return TAG_COLORS[TAG_PRESETS[tag]];
  var hash = 0;
  for (var i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i);
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length];
}

function esc(s) {
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
}

function statusIcon(s) { return s==='A'?'+':s==='D'?'−':s==='M'?'~':'·'; }
function statusCls(s) { return s==='A'?'st-add':s==='D'?'st-del':s==='M'?'st-mod':''; }

function parseDiff(diffText, fileStatus) {
  if (fileStatus === 'A') return [];
  var result = [];
  var newLineNum = 0, oldLineNum = 0, hunkHasDel = false;
  var hunkBuf = [];

  function flushHunk() {
    for (var k = 0; k < hunkBuf.length; k++) {
      var h = hunkBuf[k];
      if (h.prefix === '+') {
        result.push({ lineNum: newLineNum, type: hunkHasDel ? 'mod' : 'add', text: h.text });
        newLineNum++;
      } else if (h.prefix === '-') {
        result.push({ lineNum: oldLineNum, type: 'del', text: h.text });
        oldLineNum++;
      }
      if (h.prefix === ' ') { newLineNum++; oldLineNum++; }
    }
    hunkBuf = [];
    hunkHasDel = false;
  }

  var lines = diffText.split('\n');
  for (var i = 0; i < lines.length; i++) {
    var rawLine = lines[i];
    var m = rawLine.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (m) {
      flushHunk();
      oldLineNum = parseInt(m[1]);
      newLineNum = parseInt(m[2]);
      continue;
    }
    if (rawLine.startsWith('diff --git') || rawLine.startsWith('index ') || rawLine.startsWith('---') || rawLine.startsWith('+++')) continue;
    if (rawLine.startsWith('+') || rawLine.startsWith('-') || rawLine.startsWith(' ')) {
      var prefix = rawLine[0];
      if (prefix === '-') hunkHasDel = true;
      if (prefix !== ' ') hunkBuf.push({ prefix: prefix, text: rawLine.slice(1) });
      else { newLineNum++; oldLineNum++; }
    }
  }
  flushHunk();
  return result;
}

function render() {
  var h = '<h1>' + esc(data.project || 'SakiStep') + '</h1>';
  h += '<p class="subtitle">Progress Timeline</p>';
  if (data.entries && data.entries.length > 0) {
    var entries = data.entries.slice().reverse();
    var currentTag = null;
    entries.forEach(function(e) {
      var tag = e.tag || null;
      if (tag !== currentTag) {
        currentTag = tag;
        if (tag) {
          var c = tagColor(tag);
          h += '<div class="tag-group">';
          h += '<span class="tag-group-dot" style="background:' + c + '"></span>';
          h += '<span class="tag-group-label" style="color:' + c + '">' + esc(tag) + '</span>';
          h += '<span class="tag-group-line"></span>';
          h += '</div>';
        } else {
          h += '<div class="tag-group"><span class="tag-group-dot" style="background:#45475a"></span><span class="tag-group-label" style="color:#6c7086">其他</span><span class="tag-group-line"></span></div>';
        }
      }

      var eDiffs = diffsData && diffsData[e.id] ? diffsData[e.id] : {};
      var textFiles = (e.files||[]).filter(function(f, i){ return f.indexOf('.blend')===-1 && (e.file_status||[])[i]!=='A'; });
      var blendFiles = (e.files||[]).filter(function(f){ return f.indexOf('.blend')!==-1; });

      var tagClass = tag ? ' tagged' : '';
      var tagStyle = tag ? ';--tag-color:' + tagColor(tag) : '';
      var dateId = e.timestamp ? e.timestamp.substring(0,10) : '';
      h += '<div class="entry' + tagClass + '" style="' + tagStyle + '"' + (dateId ? ' id="d-' + dateId + '"' : '') + '>';
      h += '<div class="entry-header">';
      h += '<span class="entry-id">' + esc(e.id.substring(0,7)) + '</span>';
      h += '<span class="entry-date">' + esc(e.timestamp ? e.timestamp.substring(0,19).replace('T',' ') : '') + '</span>';
      h += '</div>';
      h += '<div class="entry-message">' + esc(e.message || '');
      if (tag) h += '<span class="tag-badge" style="background:' + tagColor(tag) + '">' + esc(tag) + '</span>';
      h += '</div>';

      if (e.files && e.files.length > 0) {
        h += '<div class="sec-hdr" onclick="toggleSec(this)"><span class="arr">▸</span><span>文件变动</span></div>';
        h += '<div class="sec-body" style="display:none">';
        e.files.forEach(function(f, i) {
          var st = (e.file_status&&e.file_status[i])||'';
          h += '<div class="fs-line ' + statusCls(st) + '"><span class="fs-badge ' + statusCls(st) + '">' + statusIcon(st) + '</span><span class="fs-path">' + esc(f) + '</span></div>';
        });
        h += '</div>';
      }

      if (textFiles.length > 0) {
        h += '<div class="sec-hdr" onclick="toggleSec(this)"><span class="arr">▸</span><span>文本改动 (' + textFiles.length + ')</span></div>';
        h += '<div class="sec-body" style="display:none">';
        if (diffsData) {
          textFiles.forEach(function(f) {
            var fi = (e.files||[]).indexOf(f);
            var fst = fi >= 0 && (e.file_status||[])[fi] ? (e.file_status||[])[fi] : 'M';
            var d = eDiffs[f];
            var lines = d ? parseDiff(d, fst) : [];
            h += '<div class="diff-entry"><div class="diff-path">' + esc(f) + '</div>';
            if (lines.length > 0) {
              lines.forEach(function(ln) {
                h += '<div class="dl ' + ln.type + '"><span class="dl-num">' + ln.lineNum + '</span><span class="dl-text">' + esc(ln.text) + '</span></div>';
              });
            } else {
              h += '<div class="diff-empty">无差异</div>';
            }
            h += '</div>';
          });
        } else {
          h += '<div class="diff-loading">加载差异中...</div>';
        }
        h += '</div>';
      }

      var eGlbs = glbData && glbData[e.id] ? glbData[e.id] : [];
      if (eGlbs.length > 0) {
        h += '<div class="sec-hdr" onclick="toggleSec(this)"><span class="arr">▾</span><span>3D 模型预览 (' + eGlbs.length + ')</span></div>';
        h += '<div class="sec-body" style="">';
        eGlbs.forEach(function(g) {
          h += '<div class="diff-entry"><div class="diff-path">' + esc(g.file) + '</div>';
          h += '<div class="model-box" data-base64="' + g.base64 + '"><div class="model-placeholder">加载 3D 模型...</div></div>';
          h += '</div>';
        });
        h += '</div>';
      }
      h += '</div>';
    });
  } else {
    h += '<div class="empty">No progress entries yet</div>';
  }
  document.getElementById('app').innerHTML = h;
}

function toggleSec(el) {
  var arr = el.querySelector('.arr');
  var body = el.nextElementSibling;
  if (body && body.classList.contains('sec-body')) {
    var isHidden = body.style.display === 'none';
    body.style.display = isHidden ? '' : 'none';
    arr.textContent = isHidden ? '▾' : '▸';
  }
}

function renderCalendar() {
  var el = document.getElementById('cal-sidebar');
  if (!el) return;
  var dates = Object.keys(calData || {}).sort();
  if (dates.length === 0) { el.style.display = 'none'; return; }

  var calYear = parseInt(new Date().getFullYear());

  function buildYear(year) {
    var h = '<div class="cal-header"><span class="cal-title">' + year + '</span><div class="cal-nav"><button onclick="calNav(-1)">&#x25C0;</button><button onclick="calNav(1)">&#x25B6;</button></div></div>';
    for (var m = 0; m < 12; m++) {
      var daysInMonth = new Date(year, m + 1, 0).getDate();
      h += '<div class="cal-month">';
      h += '<span class="cal-mlabel">' + (m + 1) + '</span>';
      h += '<span class="cal-days">';
      for (var d = 1; d <= daysInMonth; d++) {
        var ds = year + '-' + String(m + 1).padStart(2, '0') + '-' + String(d).padStart(2, '0');
        var ids = calData[ds];
        var cls = 'cal-cday';
        if (ids) cls += ids.length > 3 ? ' h4' : ids.length > 2 ? ' h3' : ids.length > 1 ? ' h2' : ' h1';
        var today = new Date();
        var isToday = ds === today.getFullYear() + '-' + String(today.getMonth() + 1).padStart(2, '0') + '-' + String(today.getDate()).padStart(2, '0');
        if (isToday) cls += ' today';
        h += '<span class="' + cls + '" title="' + ds + (ids ? ' (' + ids.length + ')' : '') + '" onclick="calScroll(\'' + ds + '\')"></span>';
      }
      h += '</span></div>';
    }
    h += '<div class="cal-legend">少<span class="s"></span><span class="s s1"></span><span class="s s2"></span><span class="s s3"></span><span class="s s4"></span>多</div>';
    el.innerHTML = h;
    calYear = year;
  }

  window.calNav = function(dir) {
    buildYear(calYear + dir);
  };

  window.calScroll = function(ds) {
    var target = document.getElementById('d-' + ds);
    if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  buildYear(calYear);
}

function createViewer(container, w, h, b64, isOverlay) {
  var scene = new THREE.Scene();
  scene.background = new THREE.Color(0x1a1a2e);
  var camera = new THREE.PerspectiveCamera(40, w / h, 0.1, 1000);
  camera.position.set(0, 0, 7);

  var renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;

  var OC = window.OrbitControls || THREE.OrbitControls;
  var controls = new OC(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.1;
  controls.target.set(0, 0, 0);
  controls.update();

  scene.add(new THREE.AmbientLight(0xffffff, 0.6));
  var keyLight = new THREE.DirectionalLight(0xffffff, 1.2);
  keyLight.position.set(5, 8, 5);
  scene.add(keyLight);
  var fillLight = new THREE.DirectionalLight(0x8888ff, 0.4);
  fillLight.position.set(-3, 2, -3);
  scene.add(fillLight);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x444444, 0.3));
  var grid = new THREE.GridHelper(10, 10, 0x444466, 0x333355);
  scene.add(grid);

  var modelObj = null;
  var originalMaterials = [];
  var wireframeVisible = false;
  var xrayMode = false;

  function saveMaterials() {
    originalMaterials = [];
    if (!modelObj) return;
    modelObj.traverse(function(c) {
      if (c.isMesh && c.material) {
        var mats = Array.isArray(c.material) ? c.material : [c.material];
        originalMaterials.push({ mesh: c, saved: mats.map(function(m) { return m.clone(); }) });
      }
    });
  }

  function rebuildWireframe() {
    if (!modelObj) return;
    modelObj.traverse(function(c) {
      if (!c.isMesh) return;
      var toRemove = [];
      c.children.forEach(function(child) {
        if (child.userData._wireframeEdge) toRemove.push(child);
      });
      toRemove.forEach(function(child) { c.remove(child); });
    });
    if (!wireframeVisible) return;
    modelObj.traverse(function(c) {
      if (!c.isMesh) return;
      try {
        var edges = new THREE.WireframeGeometry(c.geometry);
        var lines = new THREE.LineSegments(edges, new THREE.LineBasicMaterial({ color: 0x89b4fa, opacity: 0.7, transparent: true }));
        lines.userData._wireframeEdge = true;
        c.add(lines);
      } catch(e) {}
    });
  }

  function applyShading() {
    if (!modelObj) return;
    modelObj.traverse(function(c) {
      if (!c.isMesh) return;
      var info = originalMaterials.find(function(o) { return o.mesh === c; });
      if (!info) return;
      if (xrayMode) {
        var base = Array.isArray(info.saved) && info.saved.length === 1 ? info.saved[0] : info.saved[0];
        var xmat = base.clone();
        xmat.transparent = true;
        xmat.opacity = 0.35;
        xmat.wireframe = false;
        xmat.depthWrite = false;
        c.material = xmat;
      } else {
        c.material = Array.isArray(info.saved) && info.saved.length === 1 ? info.saved[0] : info.saved;
        c.material.transparent = false;
        c.material.opacity = 1.0;
        c.material.depthWrite = true;
      }
    });
  }

  function resetFrontView() {
    if (!modelObj) return;
    var box = new THREE.Box3().setFromObject(modelObj);
    var size = box.getSize(new THREE.Vector3());
    var center = box.getCenter(new THREE.Vector3());
    var dist = Math.max(size.x, size.y, size.z) * 1.8;
    camera.position.set(center.x, center.y, center.z + dist);
    controls.target.set(center.x, center.y, center.z);
    controls.update();
  }

  function showError(msg) {
    renderer.domElement.remove();
    var ph = document.createElement('div');
    ph.className = 'model-placeholder';
    ph.textContent = msg;
    container.appendChild(ph);
  }

  var binary = atob(b64);
  var bytes = new Uint8Array(binary.length);
  for (var i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  var GL = window.GLTFLoader || THREE.GLTFLoader;
  var loader = new GL();
  loader.parse(bytes.buffer, '', function(gltf) {
    modelObj = gltf.scene;
    var box = new THREE.Box3().setFromObject(modelObj);
    var size = box.getSize(new THREE.Vector3());
    var center = box.getCenter(new THREE.Vector3());
    var maxDim = Math.max(size.x, size.y, size.z);
    if (maxDim > 0) {
      var s = 3.5 / maxDim;
      modelObj.scale.set(s, s, s);
      modelObj.position.set(-center.x * s, -center.y * s + (size.y * s) / 2, -center.z * s);
      scene.add(modelObj);
      saveMaterials();
      applyShading();
      resetFrontView();
    } else {
      showError('3D 模型无可见网格');
    }
  }, function(err) {
    console.error('GLB parse failed:', err);
    showError('3D 模型解析失败');
  });

  function btn(label, cls, onclick) {
    var b = document.createElement('button');
    b.className = cls || 'mbtn';
    b.textContent = label;
    b.addEventListener('click', function(e) { e.stopPropagation(); onclick(b); });
    return b;
  }

  function makeToolbar(expandCb) {
    var tb = document.createElement('div');
    tb.className = isOverlay ? 'model-overlay-bar' : 'model-toolbar';

    var shadeModes = ['material', 'flat', 'normal'];
    var shadeLabels = { material: '贴图', flat: '灰模', normal: '平滑' };
    var currentShading = 'material';
    shadeModes.forEach(function(m) {
      var b = btn(shadeLabels[m], 'mbtn' + (currentShading===m?' active':''), function() {
        currentShading = m;
        if (modelObj) {
          modelObj.traverse(function(c) {
            if (!c.isMesh) return;
            var info = originalMaterials.find(function(o) { return o.mesh === c; });
            if (!info) return;
            if (m === 'material') {
              c.material = Array.isArray(info.saved) && info.saved.length === 1 ? info.saved[0] : info.saved;
              c.material.flatShading = false;
              c.material.needsUpdate = true;
            } else if (m === 'flat') {
              c.material = new THREE.MeshStandardMaterial({ color: 0xcccccc, roughness: 0.7, metalness: 0 });
            } else if (m === 'normal') {
              c.material = new THREE.MeshNormalMaterial();
            }
            if (xrayMode && c.material) {
              c.material.transparent = true;
              c.material.opacity = 0.35;
              c.material.depthWrite = false;
            }
          });
        }
        tb.querySelectorAll('.shade-btn').forEach(function(x) { x.classList.remove('active'); });
        b.classList.add('active');
      });
      b.classList.add('shade-btn');
      if (currentShading === m) b.classList.add('active');
      tb.appendChild(b);
    });

    tb.appendChild(document.createElement('span')).className = 'sep';

    var wireBtn = btn('线框', 'mbtn', function() {
      wireframeVisible = !wireframeVisible;
      rebuildWireframe();
      if (wireframeVisible) wireBtn.classList.add('active'); else wireBtn.classList.remove('active');
    });
    tb.appendChild(wireBtn);

    var xrayBtn = btn('透视', 'mbtn', function() {
      xrayMode = !xrayMode;
      applyShading();
      if (xrayMode) xrayBtn.classList.add('active'); else xrayBtn.classList.remove('active');
    });
    tb.appendChild(xrayBtn);

    var gridBtn = btn('网格', 'mbtn active', function() {
      grid.visible = !grid.visible;
      if (grid.visible) gridBtn.classList.add('active'); else gridBtn.classList.remove('active');
    });
    tb.appendChild(gridBtn);

    var frontBtn = btn('正视图', 'mbtn', function() {
      resetFrontView();
    });
    tb.appendChild(frontBtn);

    if (expandCb) {
      var eb = document.createElement('button');
      eb.className = 'model-expand-btn';
      eb.innerHTML = '&#x26F6;';
      eb.addEventListener('click', function(e) { e.stopPropagation(); expandCb(); });
      tb.appendChild(eb);
    }
    if (isOverlay) {
      var cb = document.createElement('button');
      cb.className = 'model-overlay-close';
      cb.innerHTML = '&times;';
      cb.addEventListener('click', function() { container.closest('.model-overlay').remove(); });
      tb.appendChild(cb);
    }
    return tb;
  }

  return { scene: scene, renderer: renderer, camera: camera, controls: controls, makeToolbar: makeToolbar, grid: grid, modelObj: modelObj, perspCam: camera };
}

function initModels() {
  if (!document.querySelector('.model-box[data-base64]')) return;
  if (typeof THREE === 'undefined' || !THREE.WebGLRenderer) {
    console.error('Three.js not loaded');
    document.querySelectorAll('.model-placeholder').forEach(function(el) {
      el.textContent = '3D 预览不可用';
    });
    return;
  }

  document.querySelectorAll('.model-box[data-base64]').forEach(function(el) {
    var b64 = el.getAttribute('data-base64');
    if (!b64) return;
    if (el.getAttribute('data-initialized')) return;
    el.setAttribute('data-initialized', '1');
    var w = el.clientWidth || 600;
    var h = 450;
    el.style.height = h + 'px';

    var v = createViewer(el, w, h, b64, false);
    el.innerHTML = '';
    el.appendChild(v.makeToolbar(function() { openFullscreen(el); }));
    el.appendChild(v.renderer.domElement);

    function anim() {
      requestAnimationFrame(anim);
      v.controls.update();
      v.renderer.render(v.scene, v.camera);
    }
    anim();
  });
}

function openFullscreen(el) {
  var b64 = el.getAttribute('data-base64');
  if (!b64) return;
  var overlay = document.createElement('div');
  overlay.className = 'model-overlay';
  var vw = window.innerWidth;
  var vh = window.innerHeight - 42;

  var v = createViewer(overlay, vw, vh, b64, true);
  var bar = v.makeToolbar(null);
  overlay.appendChild(bar);
  var canvasArea = document.createElement('div');
  canvasArea.className = 'model-overlay-canvas';
  canvasArea.appendChild(v.renderer.domElement);
  overlay.appendChild(canvasArea);
  document.body.appendChild(overlay);

  function onKey(e) {
    if (e.key === 'Escape') { overlay.remove(); document.removeEventListener('keydown', onKey); }
  }
  document.addEventListener('keydown', onKey);

  function anim() {
    if (!document.body.contains(overlay)) return;
    requestAnimationFrame(anim);
    v.controls.update();
    v.renderer.render(v.scene, v.camera);
  }
  anim();

  window.addEventListener('resize', function onResize() {
    if (!document.body.contains(overlay)) { window.removeEventListener('resize', onResize); return; }
    var nw = window.innerWidth;
    var nh = window.innerHeight - 42;
    v.perspCam.aspect = nw / nh;
    v.perspCam.updateProjectionMatrix();
    v.renderer.setSize(nw, nh);
  });
}
render();
renderCalendar();
initModels();
</script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Timeline, TimelineEntry};

    #[test]
    fn test_generate_index_html_empty() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_html_empty");
        std::fs::create_dir_all(&dir)?;

        let timeline = Timeline {
            project: "Test".to_string(),
            entries: vec![],
        };

        generate_index_html(&dir, &dir, &timeline, "Test Project")?;

        let html = std::fs::read_to_string(dir.join("index.html"))?;
        assert!(html.contains("Test Project"));
        assert!(html.contains("No progress entries yet"));
        assert!(html.contains("var diffsData = {}"));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_generate_index_html_with_entries() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_html_entries");
        std::fs::create_dir_all(&dir)?;

        let timeline = Timeline {
            project: "My Project".to_string(),
            entries: vec![TimelineEntry {
                id: "abc123def456".to_string(),
                timestamp: "2026-06-13T12:00:00Z".to_string(),
                author: "artist".to_string(),
                message: "material update".to_string(),
                files: vec!["char.blend".to_string(), "textures/diffuse.png".to_string()],
                file_status: vec![],
                preview: "previews/abc123.png".to_string(),
                tag: Some("建模".to_string()),
            }],
        };

        generate_index_html(&dir, &dir, &timeline, "My Project")?;

        let html = std::fs::read_to_string(dir.join("index.html"))?;
        assert!(html.contains("My Project"));
        assert!(html.contains("material update"));
        assert!(html.contains("abc123"));
        assert!(html.contains("建模"));
        assert!(html.contains("tag-badge"));
        assert!(html.contains("var diffsData ="));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_generate_index_html_no_tag() -> Result<()> {
        let dir = std::env::temp_dir().join("saki_html_notag");
        std::fs::create_dir_all(&dir)?;

        let timeline = Timeline {
            project: "NoTag".to_string(),
            entries: vec![TimelineEntry {
                id: "def456abc789".to_string(),
                timestamp: "2026-06-14T10:00:00Z".to_string(),
                author: "artist".to_string(),
                message: "simple fix".to_string(),
                files: vec!["fix.txt".to_string()],
                file_status: vec!["M".to_string()],
                preview: String::new(),
                tag: None,
            }],
        };

        generate_index_html(&dir, &dir, &timeline, "NoTag Project")?;

        let html = std::fs::read_to_string(dir.join("index.html"))?;
        assert!(html.contains("simple fix"));
        assert!(html.contains("tag-group"));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_transform_gltf_loader() {
        let src = std::str::from_utf8(GLTF_LOADER_SRC).unwrap();
        let result = transform_addon_js(src);
        assert!(result.starts_with("(function(){"), "should wrap in IIFE");
        assert!(result.contains("var THREE = window.THREE;"), "should have THREE alias");
        assert!(result.contains("var toTrianglesDrawMode = window.toTrianglesDrawMode;"), "should handle cross-addon import");
        assert!(result.contains("window.GLTFLoader = GLTFLoader;"), "should export GLTFLoader to window");
        assert!(!result.contains("import {"), "should have no import statements");
        assert!(!result.contains("export {"), "should have no export statements");
        assert!(!result.contains("import "), "should have no import at all");
    }

    #[test]
    fn test_base64_encode_standard() {
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_encode_binary() {
        let data: Vec<u8> = (0..255).collect();
        let b64 = base64_encode(&data);
        assert_eq!(b64.len(), ((data.len() + 2) / 3) * 4);
        assert!(b64.is_ascii());
        assert!(!b64.is_empty());
    }

    #[test]
    fn test_transform_orbit_controls() {
        let src = std::str::from_utf8(ORBIT_CONTROLS_SRC).unwrap();
        let result = transform_addon_js(src);
        assert!(result.starts_with("(function(){"), "should wrap in IIFE");
        assert!(result.contains("window.OrbitControls = OrbitControls;"), "should export OrbitControls to window");
        assert!(!result.contains("import "), "should have no import at all");
        assert!(!result.contains("export "), "should have no export at all");
    }
}