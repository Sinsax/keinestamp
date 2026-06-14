# SakiStep

Single-binary CLI + Tauri GUI Blender Git Progress Board. Rust, Edition 2024.

## Commands

```powershell
cargo build
cargo test                          # 52 unit + 7 integration tests
cargo test --lib <module>           # Single module (config/detect/init/timeline/app_config/git_helpers/...)
cargo test --test cli_tests         # Integration tests only
cargo run -- init <path>
cargo run -- commit <path> -m "msg"
cd frontend; npm run check          # svelte-check + tsc (no other lint/format config exists)
```

## Architecture

```
main ──→ init ──→ detect ──→ config
  └──→ git_project ──→ blender_render, timeline, html_gen, git_web
main_helpers ──→ config, timeline
recent ──→ independent (app-level config dir)
app_config ──→ independent (app-level config dir, blender_path)
git_helpers ──→ shared (get_changed_files, get_changed_files_status, has_blend_files, get_git_author)
```

Build order: config → detect → init → git_project → git_web → blender_render → timeline → html_gen → git_helpers → app_config → main → main_helpers → recent

## Constraints

- **Git safety**: Only `add/commit/push/pull/reset --soft`. No checkout/branch/merge/rebase/reset --hard.
- **Entry ID**: Full git commit hash. `get_entry()` supports prefix matching.
- **Config**: `.saki-step.toml` (TOML). `meta_path()` joins project root.
- **MetaConfig fields**: `version`, `init_time`, `project_root`, `web_root`, `web_remote`, `web_push_enabled`. Old fields (`project_remote`, `page_password`, `blender_path`) are backward-compatible (ignored on read via `#[serde(default)]`).
- **AppConfig**: `%APPDATA%/SakiStep/app-config.toml` (Win) or `~/.local/share/saki-step/app-config.toml` (Linux/Mac). Stores `blender_path`.
- **Atomic writes**: Temp file + rename. Temp name includes PID + target filename.
- **init idempotent**: Re-init fills gaps, does not overwrite `init_time`.
- **Chinese filenames**: `git -c core.quotepath=false` in `get_changed_files`.
- **Blender**: Not in PATH on dev machine. `render_preview` fails gracefully — commit proceeds.
- **Export**: Uses `git ls-tree --name-only` + per-file `git show` (not checkout).
- **Recent projects**: `%APPDATA%/SakiStep/recent.json` (Win) or `~/.local/share/saki-step/recent.json` (Linux/Mac). Max 20.
- **Web-timeline/**: Separate git repo, excluded from project via `.gitignore`. Pushed to Gitea via `git_web::web_commit_and_push`.
- **Git user**: `ensure_git_user()` sets fallback `user.name`/`user.email` before commit if not configured globally.
- **Decorations**: `"decorations": false` in `tauri.conf.json` — custom window chrome via `TitleBar.svelte`.
- **No duplicated code**: `get_changed_files()`, `get_changed_files_status()`, `has_blend_files()`, `get_git_author()` are in `src/git_helpers.rs` — used by both CLI and Tauri.
- **CLI output**: Chinese labels (`已提交`, `已更新`, `已撤回`, `已推送到远端`). Log format: `  {short_hash}  {date}  {author}  ({file_count} 文件)`.
- **HTML output**: Static HTML in `web-timeline/index.html`. Diff data in separate `web-timeline/diffs.json` (fetched at load time). Three.js from CDN via importmap.

## detect.rs

| Condition | Result |
|---|---|
| Path missing/not a dir | `Fresh` |
| No `.saki-step.toml`, has `.blend` | `Existing` |
| Has `.saki-step.toml` | `Initialized(MetaConfig)` |

Non-recursive scan for `*.blend` in current dir only. Non-directory path returns error.

## Integration tests

At `tests/cli_tests.rs`. Create real git repos in `$env:TEMP/cuke_int_*`. Require `user.email` and `user.name` git config (configured in `git_project` tests). Binary path via `CARGO_BIN_EXE_cuke_timeline`.

## Lib public API surface

- **config**: `meta_path`, `read_meta`, `write_meta`
- **detect**: `detect_project`
- **init**: `init_project`
- **git_project**: `add_and_commit`, `stage_files_and_commit`, `push`, `pull`, `revert`, `status`, `log`, `tracked_files`, `lfs_auto_configure`, `is_git_repo`, `ensure_git_user`, `get_current_commit_hash`, `is_lfs_available`
- **git_helpers**: `get_changed_files`, `get_changed_files_status`, `has_blend_files`, `get_git_author`
- **git_web**: `web_commit_and_push`, `is_web_repo`
- **blender_render**: `render_preview`, `export_to_glb`, `clean_old_previews`
- **timeline**: `read_timeline`, `write_timeline`, `append_entry`, `get_entry`, `remove_entry`
- **html_gen**: `generate_index_html`
- **main_helpers**: `export_entry`
- **recent**: `RecentProjects::load`, `RecentProjects::load_from`, `add_path`, `remove_path`, `paths`, `save`
- **app_config**: `AppConfig`, `default_app_config_path`, `get_blender_path`, `set_blender_path`

## Tauri GUI (src-tauri/)

`frontend/` — Svelte 5 + Vite + TypeScript (port 1420).  
`src-tauri/` — Tauri v2 Rust crate (`saki-step-tauri`) wrapping lib via `#[tauri::command]`.

### Commands

```powershell
cargo install tauri-cli           # Install Tauri CLI (once)
cargo tauri dev                   # Run Tauri dev (port 1420)
cargo tauri build                 # Production build
cd frontend; npm run tauri dev    # Same as cargo tauri dev
cd frontend; npm install          # Install JS deps (three.js for 3D viewer)
cd src-tauri; cargo build         # Build Rust backend only
cargo build                       # Root crate CLI only
```

### Tauri commands (23)

Functions in `src-tauri/src/lib.rs` wrap lib fns:

| Command | Returns | Wraps |
|---|---|---|
| `detect` | `"fresh"\|"existing"\|"initialized"` | `detect_project` |
| `init_project` | `MetaConfig` | `init_project` + `add_path` + `ensure_git_user` + initial commit |
| `commit` | short hash (7 chars) | full flow (add, commit, render, timeline, html, web push) |
| `push` / `pull` | `()` | `git_project::push` / `pull` |
| `revert` | `()` | `git_project::revert` |
| `status` | `{clean, output}` | `git_project::status` |
| `log` | `LogEntry[]` | `timeline::read_timeline` |
| `read_config` / `update_config` | `MetaConfig` / `()` | `config::read_meta` / `write_meta` |
| `export_entry` | `()` | `main_helpers::export_entry` |
| `get_timeline` | `Timeline` | `timeline::read_timeline` (resolves preview paths to absolute) |
| `get_recent` / `add_recent` / `remove_recent` | `RecentEntry[]` / `()` / `()` | `RecentProjects::load` / `add_path` / `remove_path` |
| `open_folder` | `()` | `explorer` (Win), `open` (Mac), `xdg-open` (Linux) |
| `delete_entry` | `()` | `remove_entry` + regenerate html + web commit |
| `get_project_log` | `String[]` | `git_project::log` (returns lines as array) |
| `get_tracked_files` | `String[]` | walks project dir (skips hidden/node_modules/target/web-timeline) |
| `get_file_diff` | `String` | `git show <entry_id> -- <file>` (returns diff text) |
| `get_glb_preview` | `String` | extracts .blend from git + Blender `export_to_glb` (returns path to .glb) |
| `download_blender` | `String` | downloads Blender to app data dir, saves path to AppConfig |
| `get_blender_path` | `String` | reads `blender_path` from AppConfig |
| `set_blender_path` | `()` | writes `blender_path` to AppConfig |

Note: `delete_entry` removes from `timeline.json` only — does not touch project git or files.

### Frontend components

All in `frontend/src/`:

```
App.svelte        — Main layout, state mgmt, keyboard shortcuts, tabs (Changes/Timeline)
lib/TitleBar      — Custom title bar (drag region, min/max/close, SVG cucumber logo)
lib/ProjectTabs   — Open-project tab bar (no X button)
lib/Sidebar       — Recent projects list + file tree
lib/EntryList     — Timeline entry cards with collapsible sections (files/diff/3D)
lib/StatusPanel   — Working tree file changes (gitnuro-style)
lib/DetailPanel   — Entry detail overlay
lib/ActionBar     — Bottom action buttons (commit/export/revert/push/pull)
lib/StatusBar     — Status text bar
lib/InitDialog    — Init project dialog
lib/CommitDialog  — Commit message dialog
lib/SettingsDialog — Blender path + download, web_remote, web_push_enabled
lib/Toast         — Notification toasts (auto-dismiss after 3s)
lib/DirTree       — Collapsible file tree (dirs collapsed by default)
lib/DirTreeNode   — Single file/folder tree node
lib/ModelViewer   — Three.js GLB viewer (OrbitControls)
lib/fileTree.ts   — Tree data structures: buildTree, collectDirPaths
api.ts            — Tauri invoke wrappers (23 fns)
types.ts          — TS interfaces (MetaConfig has web_remote, web_push_enabled only)
app.scss          — Global styles (CSS variables, layout, theme)
```

### Key design decisions

- **Config split**: Project-level `MetaConfig` (web_remote, web_push_enabled) vs app-level `AppConfig` (blender_path)
- **No project_remote**: Push/pull use git remote configured in project repo directly. No remote URL stored in config.
- **No page_password**: Caddy basicauth handles web access protection.
- **web_push_enabled auto-enable**: Setting `web_remote` automatically sets `web_push_enabled = true`.
- **Diff data externalized**: `web-timeline/diffs.json` loaded via fetch() to keep HTML small.
- **ActionBar**: Push/pull always enabled (no gated by remote config). Git errors shown as toasts.