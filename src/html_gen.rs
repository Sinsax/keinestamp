use crate::timeline::Timeline;
use anyhow::{Context, Result};
use std::path::Path;

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
                    glb_map.entry(entry.id.clone())
                        .or_insert_with(Vec::new)
                        .push(serde_json::json!({"file": file, "path": format!("previews_glb/{}.glb", glb_name)}));
                }
            }
        }
    }
    let glb_json = serde_json::to_string(&glb_map)?;

    let three_dir = web_root.join("three");
    ensure_threejs(&three_dir)?;

    let html = TEMPLATE
        .replace("{{TIMELINE_JSON}}", &timeline_json)
        .replace("{{GLB_JSON}}", &glb_json)
        .replace("{{DIFFS_JSON}}", &diffs_json)
        .replace("{{PAGE_TITLE}}", page_title);

    std::fs::write(web_root.join("index.html"), &html)
        .with_context(|| format!("failed to write index.html to {:?}", web_root))?;

    Ok(())
}

const THREE_VERSION: &str = "0.170.0";

fn ensure_threejs(three_dir: &Path) -> Result<()> {
    // Remove old module-based files that are no longer needed
    let old_files = ["three.module.js"];
    for old in &old_files {
        let path = three_dir.join(old);
        if path.exists() {
            eprintln!("[debug] ensure_threejs: removing old file {:?}", path);
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

    let downloads = [
        ("three.cjs", format!("https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/build/three.cjs"), false),
        ("controls/OrbitControls.js", format!("https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/examples/jsm/controls/OrbitControls.js"), true),
        ("loaders/GLTFLoader.js", format!("https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/examples/jsm/loaders/GLTFLoader.js"), true),
        ("utils/BufferGeometryUtils.js", format!("https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/examples/jsm/utils/BufferGeometryUtils.js"), true),
    ];

    for (name, url, needs_transform) in &downloads {
        let dest = three_dir.join(name);
        if dest.exists() {
            if *needs_transform {
                let content = std::fs::read_to_string(&dest).unwrap_or_default();
                if !content.starts_with("(function(){") {
                    eprintln!("[debug] ensure_threejs: re-transforming {:?} (no IIFE)", dest);
                    let _ = std::fs::remove_file(&dest);
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }
        eprintln!("[debug] ensure_threejs: downloading {} -> {:?}", url, dest);
        let tmp_path = three_dir.join(format!("{}.download", name.replace('/', "_")));
        if let Ok(()) = download_file(url, &tmp_path) {
            if *needs_transform {
                let content = std::fs::read_to_string(&tmp_path)
                    .with_context(|| format!("failed to read {:?}", tmp_path))?;
                let transformed = transform_addon_js(&content);
                let tmp2 = dest.with_extension("tmp-js");
                std::fs::write(&tmp2, &transformed)
                    .with_context(|| format!("failed to write {:?}", tmp2))?;
                std::fs::rename(&tmp2, &dest)
                    .with_context(|| format!("failed to rename to {:?}", dest))?;
                let _ = std::fs::remove_file(&tmp_path);
            } else {
                std::fs::rename(&tmp_path, &dest)
                    .with_context(|| format!("failed to rename to {:?}", dest))?;
            }
            continue;
        }
        let _ = std::fs::remove_file(&tmp_path);
        let mirror_urls = get_threejs_mirror_urls(name);
        let mut downloaded = false;
        for mirror_url in mirror_urls {
            eprintln!("[debug] ensure_threejs: trying mirror {}", mirror_url);
            if let Ok(()) = download_file(&mirror_url, &dest) {
                downloaded = true;
                break;
            }
        }
        if !downloaded {
            eprintln!("[debug] ensure_threejs: WARNING - could not download {}, 3D preview may not work", name);
        }
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
        if trimmed.starts_with("import {") {
            in_import_block = true;
            continue;
        }
        if trimmed.starts_with("import ") && (trimmed.contains(" from 'three';") || trimmed.contains(" from \"three\";")) {
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

fn get_threejs_mirror_urls(name: &str) -> Vec<String> {
    match name {
        "three.cjs" => vec![
            format!("https://mirrors.tuna.tsinghua.edu.cn/npm/three@{}/build/three.cjs", THREE_VERSION),
            format!("https://registry.npmmirror.com/three@{}/build/three.cjs", THREE_VERSION),
        ],
        "controls/OrbitControls.js" => vec![
            format!("https://mirrors.tuna.tsinghua.edu.cn/npm/three@{}/examples/jsm/controls/OrbitControls.js", THREE_VERSION),
            format!("https://registry.npmmirror.com/three@{}/examples/jsm/controls/OrbitControls.js", THREE_VERSION),
        ],
        "loaders/GLTFLoader.js" => vec![
            format!("https://mirrors.tuna.tsinghua.edu.cn/npm/three@{}/examples/jsm/loaders/GLTFLoader.js", THREE_VERSION),
            format!("https://registry.npmmirror.com/three@{}/examples/jsm/loaders/GLTFLoader.js", THREE_VERSION),
        ],
        "utils/BufferGeometryUtils.js" => vec![
            format!("https://mirrors.tuna.tsinghua.edu.cn/npm/three@{}/examples/jsm/utils/BufferGeometryUtils.js", THREE_VERSION),
            format!("https://registry.npmmirror.com/three@{}/examples/jsm/utils/BufferGeometryUtils.js", THREE_VERSION),
        ],
        _ => vec![],
    }
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let resp = ureq::get(url).call().map_err(|e| anyhow::anyhow!("download failed: {}", e))?;
    let mut reader = resp.into_reader();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|e| anyhow::anyhow!("read failed: {}", e))?;
    let tmp = dest.with_extension(format!("tmp{}_{}", std::process::id(), dest.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::write(&tmp, &buf).with_context(|| format!("failed to write {:?}", tmp))?;
    std::fs::rename(&tmp, dest).with_context(|| format!("failed to rename {:?} to {:?}", tmp, dest))?;
    Ok(())
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
.container { max-width: 800px; margin: 0 auto; padding: 40px 20px; }
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
.model-placeholder { font-size: 11px; color: #6c7086; padding: 8px; text-align: center; background: #181825; border-radius: 6px; }
.model-box { width: 100%; border-radius: 6px; overflow: hidden; background: #1a1a2e; }

</style>
</head>
<body>
<div class="container" id="app"></div>
<script>
var data = {{TIMELINE_JSON}};
var glbData = {{GLB_JSON}};
var diffsData = {{DIFFS_JSON}};

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
      h += '<div class="entry' + tagClass + '" style="' + tagStyle + '">';
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
          h += '<div class="model-box" data-src="' + g.path + '"><div class="model-placeholder">加载 3D 模型...</div></div>';
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

function initModels() {
  if (!document.querySelector('.model-box[data-src]')) return;
  if (typeof THREE === 'undefined' || !THREE.WebGLRenderer) {
    console.error('Three.js not loaded');
    document.querySelectorAll('.model-placeholder').forEach(function(el) {
      el.textContent = '3D 预览不可用';
    });
    return;
  }

  document.querySelectorAll('.model-box[data-src]').forEach(function(el) {
    var src = el.getAttribute('data-src');
    if (!src) return;
    if (el.getAttribute('data-initialized')) return;
    el.setAttribute('data-initialized', '1');
    var w = el.clientWidth || 400;
    var h = 280;
    el.style.height = h + 'px';
    el.innerHTML = '<div class="model-placeholder">⏳ 加载 3D 模型中... (' + src + ')</div>';

    var scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1a1a2e);

    var camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 1000);
    camera.position.set(3, 2, 5);

    var renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.0;

    var OrbitControls = window.OrbitControls || (THREE.OrbitControls);
    var GLTFLoader = window.GLTFLoader || (THREE.GLTFLoader);
    var controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.1;
    controls.autoRotate = true;
    controls.autoRotateSpeed = 2.0;
    controls.target.set(0, 0, 0);
    controls.update();

    var ambient = new THREE.AmbientLight(0xffffff, 0.6);
    scene.add(ambient);
    var key = new THREE.DirectionalLight(0xffffff, 1.2);
    key.position.set(5, 8, 5);
    scene.add(key);
    var fill = new THREE.DirectionalLight(0x8888ff, 0.4);
    fill.position.set(-3, 2, -3);
    scene.add(fill);
    var hemi = new THREE.HemisphereLight(0xffffff, 0x444444, 0.3);
    scene.add(hemi);
    var grid = new THREE.GridHelper(10, 10, 0x444466, 0x333355);
    scene.add(grid);

    function showError(msg) {
      renderer.domElement.remove();
      var ph = document.createElement('div');
      ph.className = 'model-placeholder';
      ph.textContent = msg;
      el.appendChild(ph);
    }

    var loader = new GLTFLoader();
    var xhr = new XMLHttpRequest();
    xhr.open('GET', src, true);
    xhr.responseType = 'arraybuffer';
    xhr.timeout = 180000;
    xhr.onprogress = function(ev) {
      if (ev.lengthComputable && ev.total > 0) {
        var pct = Math.round(ev.loaded / ev.total * 100);
        var placeholders = el.querySelectorAll('.model-placeholder');
        if (placeholders.length > 0) {
          placeholders[0].textContent = '⏳ 加载 3D 模型中... ' + pct + '% (' + Math.round(ev.loaded / 1048576) + '/' + Math.round(ev.total / 1048576) + 'MB)';
        }
      }
    };
    xhr.onload = function() {
      if (xhr.status === 200 || xhr.status === 0) {
        el.innerHTML = '';
        el.appendChild(renderer.domElement);
        loader.parse(xhr.response, '', function(gltf) {
          var model = gltf.scene;
          var box = new THREE.Box3().setFromObject(model);
          var size = box.getSize(new THREE.Vector3());
          var center = box.getCenter(new THREE.Vector3());
          var maxDim = Math.max(size.x, size.y, size.z);
          if (maxDim > 0) {
            var s = 2.5 / maxDim;
            model.scale.set(s, s, s);
            model.position.set(
              -center.x * s,
              -center.y * s + (size.y * s) / 2,
              -center.z * s
            );
            scene.add(model);
          } else {
            showError('3D 模型无可见网格');
          }
        }, function(err) {
          console.error('GLB parse failed:', src, err);
          showError('3D 模型解析失败');
        });
      } else {
        showError('3D 模型加载失败 (' + xhr.status + ')');
      }
    };
    xhr.onerror = function() {
      showError('3D 模型加载失败（网络错误）');
    };
    xhr.ontimeout = function() {
      showError('3D 模型加载超时');
    };
    xhr.send();

    function animate() {
      requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    }
    animate();
  });
}
render();
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
}