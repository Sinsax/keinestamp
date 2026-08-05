use crate::timeline::Timeline;
use crate::three_assets;
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
                let glb_rel = format!("./previews_glb/{}.glb", glb_name);
                glb_map.entry(entry.id.clone())
                    .or_insert_with(Vec::new)
                    .push(serde_json::json!({"file": file, "url": glb_rel}));
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
    three_assets::ensure_threejs(&three_dir)?;

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



const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<script>document.documentElement.dataset.theme = localStorage.getItem('keinestamp_web_theme') === 'night' ? 'night' : 'day';</script>
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{{PAGE_TITLE}} - Keinestamp</title>
<script>var module = {exports: {}}; var exports = module.exports;</script>
<script src="./three/three.cjs"></script>
<script>window.THREE = module.exports;</script>
<script src="./three/utils/BufferGeometryUtils.js"></script>
<script src="./three/controls/OrbitControls.js"></script>
<script src="./three/loaders/GLTFLoader.js"></script>
<style>
:root {
  --bg: #f4f6f8; --surface: #ffffff; --surface2: #edf0f4; --surface3: #dce1e8;
  --border: #e3e7ed; --text: #1e2833; --text-dim: #6b7684; --text-2: #506070;
  --accent: #d94747; --accent-soft: #fbeded;
  --blue: #2e4a6b; --blue-soft: #ebf1f7;
  --ze: #3e6b52; --ze-soft: #eaf2ec;
  --ice: #7fa8ce; --ice-soft: #eff5fb;
  --warning: #b0803a; --error: #c94343; --warning-soft: rgba(176,128,58,0.12);
  --card-active: #fffdfd;
  --shadow-sm: 0 1px 2px rgba(30,40,51,0.06);
  --shadow-hover: 0 4px 12px rgba(30,40,51,0.10);
}
[data-theme="night"] {
  --bg: #141d18; --surface: #1b2620; --surface2: #273830; --surface3: #35483e;
  --border: #3b4f45; --text: #f4faf6; --text-dim: #bccac4; --text-2: #c9d6d0;
  --accent: #ec6262; --accent-soft: #4a2b2d;
  --blue: #6fc49a; --blue-soft: #24392f;
  --ze: #63b58b; --ze-soft: #223a2e;
  --ice: #c3ecd4; --ice-soft: #25362e;
  --warning: #ddb25e; --error: #e96868; --warning-soft: rgba(221,178,94,0.18);
  --card-active: #233129;
  --shadow-sm: 0 1px 2px rgba(0,0,0,0.4);
  --shadow-hover: 0 4px 12px rgba(0,0,0,0.45);
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.6; }
.page { display: flex; min-height: 100vh; }
.left-col { width: 270px; flex-shrink: 0; background: var(--surface); border-right: 1px solid var(--border); display: flex; flex-direction: column; padding: 20px; position: sticky; top: 0; height: 100vh; overflow-y: auto; }
.right-col { flex: 1; min-width: 0; max-width: 800px; margin: 0 auto; padding: 40px 24px; }
h1 { font-size: 28px; font-weight: 700; margin-bottom: 8px; color: var(--text); }
.subtitle { color: var(--text-dim); margin-bottom: 32px; padding-bottom: 16px; }
.subtitle::after { content: ''; display: block; height: 2px; margin-top: 16px; border-radius: 1px; background: linear-gradient(to right, var(--accent), transparent); }
.entry { background: var(--surface); border-radius: 12px; padding: 20px; margin-bottom: 16px; border: 1px solid var(--border); border-left: 3px solid var(--border); box-shadow: var(--shadow-sm); transition: border-color .15s ease, box-shadow .15s ease, transform .15s ease; }
.entry:hover { border-color: var(--accent); box-shadow: var(--shadow-hover); transform: translateY(-1px); }
.entry.tagged { border-left-color: var(--tag-color, var(--accent)); }
.entry-header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
.entry-id { font-size: 12px; color: var(--text-dim); font-family: monospace; }
.entry-date { font-size: 14px; color: var(--text-dim); }
.entry-message { font-size: 16px; font-weight: 500; margin-bottom: 8px; color: var(--text); }
.tag-badge { display: inline-block; font-size: 11px; font-weight: 600; padding: 1px 8px; border-radius: 10px; margin-left: 8px; vertical-align: middle; color: color-mix(in srgb, var(--tag-c) 45%, var(--text)); background: color-mix(in srgb, var(--tag-c) 14%, transparent); }
.entry-files { margin-top: 6px; }
.empty { text-align: center; color: var(--text-dim); padding: 80px 0; font-size: 15px; display: flex; flex-direction: column; align-items: center; gap: 12px; }
.empty-icon { width: 72px; height: 72px; display: flex; align-items: center; justify-content: center; background: var(--ice-soft); border-radius: 50%; color: var(--ice); }
.tag-group { display: flex; align-items: center; gap: 8px; margin: 20px 0 8px; }
.tag-group-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
.tag-group-label { font-size: 13px; font-weight: 700; color: var(--text); }
.tag-group-line { flex: 1; height: 1px; background: var(--border); }

.sec-hdr { display: flex; align-items: center; gap: 5px; margin-top: 8px; padding: 5px 0; font-size: 12px; font-weight: 600; color: var(--text-dim); cursor: pointer; user-select: none; border-top: 1px solid var(--border); transition: color .15s ease; }
.sec-hdr:hover { color: var(--text); }
.sec-hdr .arr { font-size: 9px; width: 10px; flex-shrink: 0; }
.sec-body { padding: 4px 0 2px 16px; font-size: 13px; }
.fs-line { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-family: monospace; font-size: 12px; }
.fs-badge { width: 18px; text-align: center; font-weight: 700; font-size: 13px; flex-shrink: 0; }
.fs-path { color: var(--text); }
.st-add { color: var(--ze); }
.st-del { color: var(--error); }
.st-mod { color: var(--warning); }
.diff-entry { margin-bottom: 6px; }
.diff-path { font-size: 11px; font-family: monospace; color: var(--blue); padding: 2px 6px; background: var(--blue-soft); border-radius: 3px; margin-bottom: 3px; }
.diff-empty { font-size: 11px; color: var(--text-dim); padding: 2px 6px; }
.diff-loading { font-size: 11px; color: var(--text-dim); padding: 4px 6px; }
.dl { display: flex; gap: 8px; font-family: monospace; font-size: 11px; line-height: 1.6; padding: 0 4px; border-radius: 2px; }
.dl-num { width: 32px; text-align: right; flex-shrink: 0; color: var(--text-dim); font-size: 10px; user-select: none; }
.dl-text { white-space: pre; flex: 1; }
.dl.add { background: var(--ze-soft); color: var(--ze); }
.dl.add .dl-num { color: var(--ze); }
.dl.del { background: var(--accent-soft); color: var(--error); }
.dl.del .dl-num { color: var(--error); }
.dl.mod { background: var(--blue-soft); color: var(--blue); }
.dl.mod .dl-num { color: var(--blue); }
.dl:hover { filter: brightness(1.15); }
.cal { }
.cal-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
.cal-title { font-size: 14px; font-weight: 700; color: var(--text); }
.cal-month { margin-bottom: 14px; }
.cal-mlabel { font-size: 13px; font-weight: 600; color: var(--text-2); display: block; margin-bottom: 6px; }
.cal-days { display: flex; gap: 4px; flex-wrap: wrap; }
.cal-cday { width: 15px; height: 15px; border-radius: 3px; background: var(--surface3); cursor: pointer; }
.cal-cday:hover { outline: 2px solid var(--accent); outline-offset: -1px; }
.cal-cday.h1 { background: #1a6d3c; }
.cal-cday.h2 { background: #238a4a; }
.cal-cday.h3 { background: #30a758; }
.cal-cday.h4 { background: #40c468; }
.cal-cday.today { outline: 2px solid var(--accent); outline-offset: -1px; }
.cal-legend { display: flex; align-items: center; gap: 4px; margin-top: 12px; font-size: 11px; color: var(--text-dim); }
.cal-legend .s { width: 14px; height: 14px; border-radius: 3px; background: var(--surface3); display: inline-block; }
.cal-legend .s.s1 { background: #1a6d3c; }
.cal-legend .s.s2 { background: #238a4a; }
.cal-legend .s.s3 { background: #30a758; }
.cal-legend .s.s4 { background: #40c468; }
.left-footer { margin-top: auto; display: flex; flex-direction: column; gap: 4px; padding-top: 12px; border-top: 1px solid var(--border); }
.left-footer-btn { display: flex; align-items: center; justify-content: center; gap: 6px; padding: 8px; border-radius: 6px; background: var(--surface2); border: 1px solid var(--border); color: var(--text); cursor: pointer; font-size: 12px; font-weight: 600; transition: background 0.12s; }
.left-footer-btn:hover { background: var(--surface3); }
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
    <div class="brand">
      <svg viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M28 22 C44 14, 56 14, 72 22 C84 34, 84 66, 72 78 C56 86, 44 86, 28 78 C16 66, 16 34, 28 22Z" fill="#D94747"/>
        <path d="M33 28 C22 40, 22 60, 33 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
        <path d="M44 24 C32 40, 32 60, 44 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
        <path d="M56 24 C44 40, 44 60, 56 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
        <path d="M67 28 C56 40, 56 60, 67 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
        <circle cx="40" cy="40" r="3" fill="#FFE3E3"/>
        <circle cx="44" cy="55" r="3.5" fill="#9FC3E3"/>
        <circle cx="50" cy="70" r="3" fill="#FFE3E3"/>
        <circle cx="52" cy="38" r="3.5" fill="#9FC3E3"/>
        <circle cx="58" cy="53" r="3" fill="#FFE3E3"/>
        <circle cx="62" cy="68" r="3.5" fill="#9FC3E3"/>
        <path d="M46 16 C48 10, 52 6, 58 8" stroke="#E8B4B4" stroke-width="3" fill="none" stroke-linecap="round"/>
        <path d="M58 8 C62 6, 64 8, 66 7" stroke="#E8B4B4" stroke-width="2.5" fill="none" stroke-linecap="round"/>
      </svg>
      <span class="brand-name">{{PAGE_TITLE}}</span>
    </div>
    <div id="cal-sidebar"></div>
    <div class="left-footer">
      <button id="theme-toggle" class="left-footer-btn" style="margin-top:6px">☀ 切换日/夜</button>
      <span style="font-size:10px;color:var(--text-dim);text-align:center;margin-top:8px">Keinestamp</span>
    </div>
  </div>
  <div class="right-col">
    <div id="app"></div>
  </div>
</div>
<script>
document.getElementById('theme-toggle').addEventListener('click', function() {
  var next = document.documentElement.dataset.theme === 'night' ? 'day' : 'night';
  document.documentElement.dataset.theme = next;
  localStorage.setItem('keinestamp_web_theme', next);
});
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
  var h = '<h1>' + esc(data.project || 'Keinestamp') + '</h1>';
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
          h += '<div class="tag-group"><span class="tag-group-dot" style="background:var(--border)"></span><span class="tag-group-label" style="color:var(--text-dim)">其他</span><span class="tag-group-line"></span></div>';
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
      if (tag) h += '<span class="tag-badge" style="--tag-c:' + tagColor(tag) + '">' + esc(tag) + '</span>';
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
          h += '<div class="model-box" data-url="' + g.url + '"><div class="model-placeholder">加载 3D 模型...</div></div>';
          h += '</div>';
        });
        h += '</div>';
      }
      h += '</div>';
    });
  } else {
    h += '<div class="empty"><div class="empty-icon"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor"><path d="M12 2a10 10 0 100 20 10 10 0 000-20zm0 2a8 8 0 110 16 8 8 0 010-16zm-1 2v6.586l4.707 4.707 1.414-1.414L13 11.586V6h-2z"/></svg></div>暂无记录</div>';
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

  var dateMsgs = {};
  if (data && data.entries) {
    data.entries.forEach(function(e) {
      var ds = e.timestamp ? e.timestamp.substring(0,10) : '';
      if (!ds) return;
      if (!dateMsgs[ds]) dateMsgs[ds] = [];
      if (dateMsgs[ds].indexOf(e.message) === -1) dateMsgs[ds].push(e.message);
    });
  }

  var first = dates[0].split('-').map(Number);
  var startY = first[0], startM = first[1];
  var endM = startM + 5, endY = startY;
  if (endM > 12) { endM -= 12; endY += 1; }

  var title = startY + '-' + String(startM).padStart(2, '0') + ' ~ ' +
              endY + '-' + String(endM).padStart(2, '0');
  var h = '<div class="cal-header"><span class="cal-title">' + title + '</span></div>';

  for (var i = 0; i < 6; i++) {
    var cur = startY * 12 + (startM - 1) + i;
    var y = Math.floor(cur / 12);
    var m = cur % 12 + 1;
    var daysInMonth = new Date(y, m, 0).getDate();
    h += '<div class="cal-month">';
    h += '<span class="cal-mlabel">' + m + '月</span>';
    h += '<span class="cal-days">';
    for (var d = 1; d <= daysInMonth; d++) {
      var ds = y + '-' + String(m).padStart(2, '0') + '-' + String(d).padStart(2, '0');
      var ids = calData[ds];
      var cls = 'cal-cday';
      if (ids) cls += ids.length > 3 ? ' h4' : ids.length > 2 ? ' h3' : ids.length > 1 ? ' h2' : ' h1';
      var today = new Date();
      var isToday = ds === today.getFullYear() + '-' + String(today.getMonth() + 1).padStart(2, '0') + '-' + String(today.getDate()).padStart(2, '0');
      if (isToday) cls += ' today';
      var tipLines = [ds];
      if (ids) tipLines.push('(' + ids.length + ' 条)');
      var msgs = dateMsgs[ds] || [];
      msgs.slice(0, 3).forEach(function(m) { tipLines.push('• ' + m); });
      if (msgs.length > 3) tipLines.push('… 还有 ' + (msgs.length - 3) + ' 条');
      var tip = tipLines.join(' &#10; ');
      h += '<span class="' + cls + '" title="' + tip + '" onclick="calScroll(\'' + ds + '\')"></span>';
    }
    h += '</span></div>';
  }
  h += '<div class="cal-legend"><span title="较少">少</span><span class="s" title="0 条"></span><span class="s s1" title="1 条"></span><span class="s s2" title="2 条"></span><span class="s s3" title="3 条"></span><span class="s s4" title="4+ 条"></span><span title="较多">多</span></div>';
  el.innerHTML = h;
}

window.calScroll = function(ds) {
  var target = document.getElementById('d-' + ds);
  if (target) {
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    target.style.transition = 'background 0.3s';
    target.style.background = 'rgba(137,180,250,0.12)';
    setTimeout(function() { target.style.background = ''; }, 2000);
  }
};

function createViewer(container, w, h, url, isOverlay) {
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

  var GL = window.GLTFLoader || THREE.GLTFLoader;
  var loader = new GL();
  loader.load(url, function(gltf) {
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
  }, null, function(err) {
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

var ACTIVE_VIEWERS = [];
var VIEWER_QUEUE = [];
var OBSERVER = null;
var VIEWER_ID = 0;

function initModelsLazy() {
  var boxes = document.querySelectorAll('.model-box[data-url]');
  if (boxes.length === 0) return;
  if (typeof THREE === 'undefined' || !THREE.WebGLRenderer) {
    document.querySelectorAll('.model-placeholder').forEach(function(el) {
      el.textContent = '3D 预览不可用';
    });
    return;
  }
  OBSERVER = new IntersectionObserver(function(entries) {
    entries.forEach(function(e) {
      var el = e.target;
      if (e.isIntersecting && !el.getAttribute('data-viewer-id')) {
        enqueueViewer(el);
      } else if (!e.isIntersecting && el.getAttribute('data-viewer-id')) {
        disposeViewer(el);
      }
    });
  }, { rootMargin: '200px' });
  boxes.forEach(function(el) {
    el.style.height = '450px';
    el.innerHTML = '<div class="model-placeholder">等待加载...</div>';
    OBSERVER.observe(el);
  });
}

function enqueueViewer(el) {
  if (ACTIVE_VIEWERS.length < 5) {
    createViewerFor(el);
  } else {
    VIEWER_QUEUE.push(el);
    el.innerHTML = '<div class="model-placeholder">排队中 (' + VIEWER_QUEUE.length + ')...</div>';
  }
}

function createViewerFor(el) {
  var id = ++VIEWER_ID;
  el.setAttribute('data-viewer-id', '' + id);
  var url = el.getAttribute('data-url');
  var w = el.clientWidth || 600;
  var v = createViewer(el, w, 450, url, false);
  el.innerHTML = '';
  el.appendChild(v.makeToolbar(function() { openFullscreen(el); }));
  el.appendChild(v.renderer.domElement);
  var entry = { id: id, el: el, viewer: v, animId: null };
  ACTIVE_VIEWERS.push(entry);
  function anim() {
    var found = ACTIVE_VIEWERS.findIndex(function(x) { return x.id === id; });
    if (found === -1 || !entry.el.isConnected) return;
    entry.animId = requestAnimationFrame(anim);
    v.controls.update();
    v.renderer.render(v.scene, v.camera);
  }
  anim();
}

function disposeViewer(el) {
  var id = el.getAttribute('data-viewer-id');
  if (!id) return;
  el.removeAttribute('data-viewer-id');
  var idx = ACTIVE_VIEWERS.findIndex(function(v) { return v.id === +id; });
  if (idx !== -1) {
    var entry = ACTIVE_VIEWERS[idx];
    cancelAnimationFrame(entry.animId);
    entry.viewer.renderer.dispose();
    ACTIVE_VIEWERS.splice(idx, 1);
  }
  el.innerHTML = '<div class="model-placeholder">3D 已暂停，滚动回来看</div>';
  if (VIEWER_QUEUE.length > 0) {
    var next = VIEWER_QUEUE.shift();
    if (next && next.isConnected && !next.getAttribute('data-viewer-id')) {
      createViewerFor(next);
    }
  }
}

function openFullscreen(el) {
  var url = el.getAttribute('data-url');
  if (!url) return;
  var overlay = document.createElement('div');
  overlay.className = 'model-overlay';
  var vw = window.innerWidth;
  var vh = window.innerHeight - 42;

  var v = createViewer(overlay, vw, vh, url, true);
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
initModelsLazy();
</script>
</body>
</html>"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Timeline, TimelineEntry};

    #[test]
    fn test_generate_index_html_empty() -> Result<()> {
        let dir = std::env::temp_dir().join("keinestamp_html_empty");
        std::fs::create_dir_all(&dir)?;

        let timeline = Timeline {
            project: "Test".to_string(),
            entries: vec![],
        };

        generate_index_html(&dir, &dir, &timeline, "Test Project")?;

        let html = std::fs::read_to_string(dir.join("index.html"))?;
        assert!(html.contains("Test Project"));
        assert!(html.contains("暂无记录"));
        assert!(html.contains("empty-icon"));
        assert!(html.contains("var diffsData = {}"));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_generate_index_html_with_entries() -> Result<()> {
        let dir = std::env::temp_dir().join("keinestamp_html_entries");
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
        let dir = std::env::temp_dir().join("keinestamp_html_notag");
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
        let src = std::str::from_utf8(crate::three_assets::GLTF_LOADER_SRC).unwrap();
        let result = crate::three_assets::transform_addon_js(src);
        assert!(result.starts_with("(function(){"), "should wrap in IIFE");
        assert!(result.contains("var THREE = window.THREE;"), "should have THREE alias");
        assert!(result.contains("var toTrianglesDrawMode = window.toTrianglesDrawMode;"), "should handle cross-addon import");
        assert!(result.contains("window.GLTFLoader = GLTFLoader;"), "should export GLTFLoader to window");
        assert!(!result.contains("import {"), "should have no import statements");
        assert!(!result.contains("export {"), "should have no export statements");
        assert!(!result.contains("import "), "should have no import at all");
    }

    #[test]
    fn test_transform_orbit_controls() {
        let src = std::str::from_utf8(crate::three_assets::ORBIT_CONTROLS_SRC).unwrap();
        let result = crate::three_assets::transform_addon_js(src);
        assert!(result.starts_with("(function(){"), "should wrap in IIFE");
        assert!(result.contains("window.OrbitControls = OrbitControls;"), "should export OrbitControls to window");
        assert!(!result.contains("import "), "should have no import at all");
        assert!(!result.contains("export "), "should have no export at all");
    }
}