# GUI guide

**English** | [简体中文](zh-CN/gui-guide.md)

This guide covers day-to-day use: from creating a project to recording, rolling back, backing up and
deploying the web timeline. For the server and the command line see the
[server deployment guide](server-guide.md) and the [CLI reference](cli.md).

> Keinestamp is a general-purpose progress board for binary projects, currently focused on Blender
> workflows. Other project types (textures, videos, design sources, …) follow the same flow
> (record / rollback / backup / deploy); preview support per file type is listed in the
> [Scope section of the README](../README.md#scope).

---

## 1. Install and launch

- Requirements: Windows 10/11 x64
- Two ways to install:
  - **Installer**: run `Keinestamp_<version>_x64-setup.exe`
  - **Portable**: extract `Keinestamp-<version>-windows-x64-portable.zip` anywhere and run
    `Keinestamp.exe`

> The portable build requires `Keinestamp.exe` and `lore-bin/` in the same directory.
> At startup the app verifies the bundled Lore version and requires `0.8.6`; if it is missing or
> mismatched a dialog appears and actions such as record / rollback are disabled.

**Closing the window**: the first close asks once; afterwards the app follows your setting —
“stay in the background (notification area)” or “quit”.
Left-click the tray icon to reopen the window; right-click to quit.

---

## 2. Projects

### New project

Click `+` at the far right of the title bar and pick a directory:

- Empty directory: initializes a brand-new project
- Directory containing `.blend` files: adopted as-is (files are never moved or deleted)
- An already initialized project is detected and opened directly, without re-initializing

Initialization creates:

```
<project>/
├── .lore/                  # Lore repository (history, object store)
├── .keinestamp/            # app-private directory (not versioned): web output, backups, config
└── .loreignore             # ignore rules (ignores .keinestamp/ by default)
```

### Opening an existing project

- Projects opened before appear as **project tabs** in the title bar; click to switch
- Settings → General → “Open the last project on startup” restores it on the next launch
- Projects downloaded from a remote: Settings → Server / Remote repositories → after downloading
  click “Open”

---

## 3. Main window

```
┌ Title bar: project tabs / theme toggle / settings / window buttons ──────┐
│ Changes panel (left)             │ Timeline (right)                     │
│  tick / select all / refresh     │  search / process filter / cards     │
│  ignore rules                    │  details: file changes / previews    │
├ Bottom action bar: Record · Restore · Backup · Deploy ──────────────────┤
└ Status bar: connection state / project info ────────────────────────────┘
```

- **Changes panel**: lists uncommitted changes in the workspace. The checkboxes decide which files
  are included in this record; ticking a directory includes everything under it. Ignored files are
  still shown (greyed out) and can be un-ignored from the context menu.
- **Timeline**: each record is a card showing the message, time, process / tags, file count and 3D
  preview badge.
- **Bottom action bar**: Record / Restore / Backup / Deploy. Hover a disabled action to see why.

---

## 4. Recording progress

1. In the **Changes** panel tick the files for this record (at least one; a message-only record is
   also allowed)
2. Click **Record** in the bottom bar
3. In the dialog fill in:
   - **Message** (required)
   - **Process category** (optional): processes from the project configuration
   - **Tags** (optional): pick presets or click “Custom” to type one
   - **Generate preview** (optional): build web 3D previews for this record (requires Blender)
4. Confirm and wait; the new record appears at the top of the timeline

### Marking 3D previews

- Right-click a `.blend` file in the Changes panel → “3D preview”; a blue 3D badge appears on that
  row
- Only **marked** `.blend` files are exported to GLB during a record (unmarked ones are skipped to
  save time)
- Marking only affects future records; the 3D badge on a timeline card opens the preview, and
  right-clicking it toggles the preview for that record

### Discarding changes of a single file

Right-click a file in the Changes panel → “Discard changes” restores it to the latest record
(with a confirmation). Untracked files are not supported here — just delete them.

### Ignoring

- Right-click a file / directory / extension → “Ignore file / Ignore directory / Ignore format”
- Rules are written to `.loreignore` in the project root (versioned with the repository, gitignore
  semantics)
- Ignored files stay visible in grey; right-click → “Unignore” to undo
- The “Ignore rules” section under the Changes panel lets you add or remove rules by hand

---

## 5. Timeline

- **Search**: matches record messages, processes, tags and file names
- **Process filter**: the dropdown at the top filters by process
- **Milestones**: the ★ on a card marks / unmarks a milestone
- **Details**: clicking a card shows ID, time, process, tags, file count, preview count and file
  changes on the right
- **Context menu**:
  - Export (extract that record's version to a directory)
  - Compare with previous record (file differences)
  - Roll back
  - Toggle preview
- **Open the local timeline page**: view this project's web timeline in a browser (no deployment
  needed)

---

## 6. Rollback and restore

Rollback in Keinestamp means “move the workspace back to before a record”; history is not deleted.

### Rolling back

Right-click a record **X** → Roll back:

1. The workspace goes back to **X's parent version** (the state before X)
2. **X's changes** are checked out as uncommitted modifications:
   - Modified / added files: state “rollback change”; they can be ticked, edited and re-recorded
   - Deleted files: state “rollback deletion”, read-only; re-recording commits them as deletions
3. Uncommitted changes that existed before the rollback are stashed in
   `.keinestamp/rollback-backup/` and restored when you restore
4. New untracked files are moved out and stashed too, so nothing is lost

### Consecutive rollbacks

Rolling back again while already in a rollback state only backs up the **first** rollback.
If a checked-out file was edited by you (“record version · edited”), a dialog asks
“Discard and continue / Keep”; pure checkouts (“record version”) are skipped silently.

### Restore (undo rollback)

The **Restore** button in the bottom bar:

- The workspace returns to the HEAD from before the rollback, and the stash is restored
- If current changes conflict with the pre-rollback stash, you are asked per file whether to keep
  the “workspace version after rollback” or the “stashed version before rollback”
- If there are real modifications, you are asked to confirm discarding them

### Superseding

Recording while in a rollback state: the new record's parent is the rollback point, and records
after X are superseded out of the mainline (still kept in the data, shown collapsed). This is
expected behaviour.

---

## 7. Backup (LAN loreserver)

Backup = `lore push` to a LAN loreserver.

1. Settings → Server: enter the loreserver **IP / port** (default `41337`) and save
2. Click **Backup** in the bottom bar
3. If the project was created offline (no remote), you are offered “Migrate and back up”: the app
   registers the repository on the server, pushes everything, and converts it to an online
   repository

> - The backup server has no authentication (`auth: None`); **trusted LAN only**
> - Migration is refused while in a rollback state; restore first
> - The backup server comes from `serve`'s optional loreserver or a standalone `loreserver`; see
>   the server guide

---

## 8. Web deployment

Publishes the timeline as a website (browser access with login).

1. Settings → Server: enable “Web deployment service”, fill in the server address / port, save
2. In the “Site” area on the right of the timeline:
   - **Site name**: ASCII letters, digits, `-`, `_` only; locked after deployment (it is the
     `/<name>/` path)
   - **Access token**: used for browser login; can be generated randomly, effective after saving
3. Click **Deploy** in the bottom bar and wait for the upload
4. After a successful deploy click “Open site” to view it in the browser

> The deployment connection is independent of the deployment state: a project that has never been
> deployed can still be deployed for the first time.
> When local changes are newer than the deployed version, the button hints “a newer version can be
> deployed”.

---

## 9. Settings

Four tabs: **Blender · Tags · Server · About** (tab labels are shorthand; details below).

### 9.1 Blender (including General)

- **Blender**: version list, download and install, delete, switch the active version
- **General**:
  - Open the last project on startup
  - Commit identity (author shown on records; app-wide global; any name, `@local` not required)
  - Behaviour when closing the window (stay in tray / quit)

### 9.2 Tags (processes / tags, per project)

- Each process has a name and a set of tags; by default there is a single “Blender” process
- Add / delete processes, add / remove tags, restore defaults
- Saving takes effect immediately in the record dialog and the timeline filter
- Stored in the project's `.keinestamp/tags.toml`, **effective for the current project only**; not
  editable while no project is open

### 9.3 Server (including remote repositories)

- Service switches: web deployment service, loreserver service
- Address / port: web deployment and loreserver may share an IP
- Deploy token: upload and health-check authentication
- Test connection: probes both services separately
- Remote repositories: lists repositories on the server; download, delete, or open a
  previously downloaded local project
- Test the connection automatically when opening a project

### 9.4 About

- **Version / platform**: the version injected at build time (same source as `Cargo.toml` /
  `tauri.conf.json`) and the running platform
- **Lore**: shows only the bundled `lore` version; red with an explanation when missing or not
  `0.8.6` (the error message includes the actual path)
- **Locations**: program directory and config directory, each with a one-click open in Explorer
- **Updates**: shows the source repository; entering the About tab checks GitHub Releases once, and
  “Check for updates” retries. When a newer version exists the status line shows a clickable
  “New version vX.Y.Z found ↗” that opens that release page; a small dot also appears next to the
  About tab
- When offline / rate-limited it honestly shows “unknown” and does not affect the other settings
- The About tab is read-only and has no bottom buttons (click outside the dialog to close)

---

## 10. Data locations and troubleshooting

| Content | Location |
|---|---|
| Project history / objects | `<project>/.lore/` |
| Web output, timeline cache | `<project>/.keinestamp/web/` |
| Rollback stash | `<project>/.keinestamp/rollback-backup/` |
| Discard backups (last 5) | `<project>/.keinestamp/discard-backup-*/` |
| Global settings | `%APPDATA%\Keinestamp\app-config.toml` |

Common issues:

- **Lore version mismatch**: make sure `lore-bin/` is complete (`lore.exe`, `loreserver.exe`,
  `version.txt`, `LICENSE`) and version `0.8.6`
- **Rollback asks for discard confirmation**: a checked-out file was edited; choose “Discard and
  continue” or back up your edits first
- **Restore reports conflicts**: current changes conflict with the pre-rollback stash; choose which
  side to keep per file
- **Backup says the repo was created offline**: follow “Migrate and back up”, or register a
  same-name repository on the server first
- **Deploy returns 403**: the project directory exists but the registry has no identity; ask the
  server admin to clean it up and deploy again
- **Address not found**: history may have been cleaned up or not fully downloaded yet; `sync` or
  re-clone first

---

## 11. Language

Settings → General → **Language**, three options:

| Option | Behaviour |
|---|---|
| Follow system (default) | Uses the OS language; Chinese systems = Simplified Chinese, others = English |
| 简体中文 | Always Chinese |
| English | Always English |

Switching takes effect **immediately** (no restart): main window, changes panel, timeline and tray
menu all update.
Deployed web pages are **always English** and are not affected by this setting; user data (project
names, process names, tag names, file names, commit messages) is shown verbatim and never
translated.

Changing the OS language itself requires an app restart to be picked up. Scripts or CI that need a
fixed language can set `KEINESTAMP_LOCALE=zh-CN` (or `en-US`).
