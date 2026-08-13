# Keinestamp v2（Lore 后端）

Single-binary CLI + Tauri GUI Blender Git Progress Board，后端为 Epic Games **Lore** 0.8.6（分块去重 VCS）。Rust Edition 2024。

## Commands

```powershell
cargo test                          # 99 unit + 11 integration
cargo test --lib <module>           # Single module (config/detect/init/lore_backend/lore_project/...)
cargo test --test cli_tests         # Integration only
cargo run -- init <path>            # CLI entry
cd src-tauri; cargo build           # Tauri backend only
cd frontend; npm run check          # svelte-check + tsc (no other lint/format)
```

**Lore 依赖**：**构建时自动捆绑**（build.rs 按平台下载 v0.8.6 的 `lore`+`loreserver` 到 `src-tauri/resources/lore-bin/`，sha256 校验，缺失/失败 → 构建失败；**自动拷贝到构建产物目录 `target/<profile>/lore-bin`（exe 同目录）**；Tauri resources 打包、CLI 分发 = exe + 同目录 lore-bin 两件套）。**国内网络/离线构建**（build.rs 支持，按优先级）：① `KEINESTAMP_LORE_ARCHIVE_DIR` = 本地目录放两份官方资产压缩包（离线构建，仍 sha256 校验）；② `KEINESTAMP_LORE_RELEASE_BASE` = 自定义镜像 base（布局同 GitHub Releases）；③ 默认官方 GitHub → 公共加速代理（gh-proxy.com 等）回退，每源重试 3 次；④ 从其他机器拷贝整份 `lore-bin` 到 `src-tauri/resources/lore-bin/` 直接跳过下载。运行时查找：`KEINESTAMP_LORE_BIN` env → exe 同目录 `lore-bin/` → Tauri resource_dir 注入 → 平台默认（Windows `C:\Users\para\AppData\Local\lore\bin\lore.exe` / Linux `lore` PATH）。**启动检测**（`check_lore_env`：`--version` 精确 == 0.8.6）：CLI 命令前、serve 启动（拒绝启动）、GUI 启动（弹窗 + ActionBar 操作禁用）。Lore 为 MIT 许可（© Epic Games, Inc.），捆绑附带 LICENSE。**tauri.conf.json 的 resources glob 用 `*`（`**` 不被 tauri 接受）；文件勿用 PowerShell Set-Content 写（会加 BOM 破坏 JSON）**。

## Architecture

```
main ──→ init ──→ detect ──→ config
  └──→ lore_project ──→ lore_backend（lore --offline，纯本地）
            ├──→ hidden (app-config 隐藏列表)
            ├──→ timeline (展示缓存 web/timeline.json)
            └──→ html_gen → web/（磁盘产物，不入库）
main_helpers ──→ config, timeline
recent, app_config ──→ independent (app-level config dir)
serve ──→ 多租户 HTTP（web 部署；loreserver 拉起已移除，局域网备份后续按需补）
```

Build order (crate): config → detect → init → lore_backend → lore_project → hidden → timeline → html_gen → app_config → main → main_helpers → recent

## Constraints

- **Lore CLI 锁版本 v0.8.6**（Windows x86_64 MSVC），不得用 latest；命令形态以 `docs/lore-commands.md`（Task 1 实测速查表）为准，禁止凭空猜参数
- **纯本地离线**：`create_repo` = `lore --offline repository create <随机32hex>`（无服务器注册；仓库名限 ASCII，目录可任意含中文）；config.toml remote_url 为空 → push 报 "No remote configured"（局域网备份后续按需补）
- **lore identity（提交身份）**：在线仓库 commit 需要 identity（离线创建留空；转在线后 lore 依赖服务器解析身份，**服务器离线即记录失败**）→ **程序级全局**（AppConfig `lore_identity`，默认 `keinestamp@local`，设置-通用可改；CLI `config --set identity=xxx`）；`commit_staged` 统一带 `lore --identity <值> commit`（设置即生效、不写仓库 config、不依赖服务器）；配置身份后 `lore history` 的 message 块附加 `Creator/Committer` 行 → `revision_history` 解析过滤
- **lore 调用集中点**：所有 `lore`/`loreserver` 命令必须经 `src/lore_backend.rs`（唯一 spawn 点），其他模块不得直接调
- **staged 残留清理**（`unstage_all`，2026-08-12）：lore commit = 提交**全部 staged**（非指定集）——任何异常残留（回退失败中间态等）会被下次 commit 一并提交（"没勾选也被记录"根因）→ `commit_files`/`commit`/`init_commit` 的 **stage 前先 `unstage_all`**（status 解析 "Changes staged for commit" 段逐文件 `lore unstage`；**首字符判断状态码**（char 数组 strip_prefix 是序列匹配非任一成员）+ **路径 trim 尾随空格**（status 行格式 `A <path> `）——实测 lore 路径带尾随空格 unstage 无效）；`.loreignore` 自动 stage 由 `commit_files` 统一（record 不再自行 stage）
- **目录路径不 stage**（2026-08-12 实测）：`lore stage <目录>` = stage **目录下全部 dirty 文件**（docs:108「目录只 stage 已 dirty 文件」）；lore status 输出**含目录行**（`A modeling/`）→ 前端 statusFiles/selection 含目录路径——**"没勾选被记录"真根因**：取消勾选目录时 `toggleDir` 只移除目录下文件、`modeling/` 本身残留 → 记录时 stage 目录 = 全提 → **四层防御**：① `record` 的 paths `retain(!ends_with('/'))`（目录丢弃）；② `commit_files` stage 参数同样过滤目录；③ 前端 `toggleDir` 取消时同时移除 `node.path`（选中时补回）；④ `handleRecord` 的 files 过滤 `!endsWith('/')`
- **file reset/write 不创建父目录**（2026-08-12 实测 os error 2）：回退到 init（树 = 仅 .loreignore）后工作区目录被 branch reset 清空 → 检出嵌套路径文件（`refer/back.png`）父目录不存在 → `lore file reset` 报 "系统找不到位置/os error 2/unable to restore path" → **检出前对每个路径 `create_dir_all` 父目录**（rollback_inner 的 file reset + `write_back` 函数内；export_files 一直手动建，佐证 lore 不建）；`rollback_modified_files` 过滤目录行（`A refer/` 会被误判"真修改"导致恢复需确认）
- **回退检出统一语义**（2026-08-12 定稿）：change_files = **所选版本（X）树的全部文件**（`dump --revision X`，过滤 .keinestamp/.loreignore/目录行）——**不做忽略特例**：忽略文件/两版共有文件都算检出（`file write` 从 X 写回覆盖工作区残留——**忽略文件也检出为 X 版本**，branch reset 不处理忽略文件）；`revision_changes` 仅用于 D（deleted_files）；`write_back` = create_dir_all + **先 remove_file 再 write**（`file write --output` 不允许覆盖已存在文件，实测 invalid path）；恢复 = branch_reset(head_before) + **丢弃项（exclude_new）∩ head_before 树文件写回**（覆盖忽略文件残留）；**`repository dump`/`revision diff` 的目录行格式不同**（dump 带 `/`、diff 无 `/`——`A refer`）→ 目录行过滤用 **`/` 结尾 + 前缀匹配双保险**（`dump_tree_files`）；前端 visibleFiles 补充 change_files（回退态显示全部检出文件——含 status 无变化的）
- **revision 定位**：必须用签名（完整或前缀 hash），数字 N 不被接受；数字→签名用 `lore revision find number N`
- **metadata**：用户提交 `kind=user`；tag/milestone/message 语义以 revision metadata 为准；timeline.json 仅展示缓存（web 部署用，可重建）
- **隐藏/忽略机制**（替代 gitignore/skip-worktree）：
  - 用户自定义忽略（文件/格式/目录）→ **项目级 `.loreignore`**（Lore 原生过滤 status/stage；规则随仓库历史，与 gitignore 同语义；文件须无 BOM）；**被忽略文件在变更面板/文件树仍显示**（灰色 + 「已忽略」标记，可右键取消忽略——`get_tracked_files` walk 不过滤 loreignore，StatusPanel `visibleFiles` 对规则匹配文件灰显加入）
  - **程序私有目录 `.keinestamp/` 整体隔离**（2026-08-12 定稿）：`ensure_loreignore` 预设写入 `.keinestamp/`（lore 原生完全忽略——status/stage/branch reset 校验全不碰；程序只追踪项目内容）。web 产物（html_gen/timeline.json/previews_glb）由磁盘生成、部署 zip 上传，**不入库**（`commit_web` 已删除）；web 版本号 = `.keinestamp/web-version` 本地计数器（`web_revision_count`/`bump_web_version`，产物重建时自增，push.rs X-Web-Count 与 get_deploy_status 读它）；`load_hidden` 的 `.keinestamp/` 前缀过滤保留作双保险（用户删掉 loreignore 行时仍隐藏）
  - 回退/恢复相关：`revision_changes` 过滤 `.keinestamp/` 前缀 + `.loreignore`（程序产物不检出；旧项目历史含 web 提交时防御，实测 36_mine2 旧记录回退正常）；**branch reset 前 `reset_loreignore_to`**（`file reset --revision <目标> .loreignore`——自动记录文件 hidden 过滤看不到 dirty，但 lore 校验其内容）
  - **web 只显示主线**：html_gen 模板 JS 渲染过滤 `superseded`（`mainline = entries.filter(e => !e.superseded)`）；timeline.json 数据保留 superseded（GUI 折叠区/回退需要）；serve 静态托管 html_gen 产物（无需另改）
  - **条目删除 tombstone**（2026-08-12）：`Timeline.removed_entries`（delete_entry 记录被删 id；`rebuild_cache` 重建时过滤——否则每次记录后把被删条目拉回）
  - **init 清理 `*.~loretemp`**（lore 原子写中断残留的临时文件，递归 walk 删除）
  - app-config 的 hidden_paths 已退役
- **回退模型**（v1 语义定稿）：
  - **选 X 回退 = 回退到 X 的父（X-1）**：`branch reset X-1`；X 相对 X-1 的改动（`revision diff` 解析 A/M/D）检出为**真实未提交修改**：
    - M/A 文件：`lore file reset --revision X` 检出（状态"回退改动"，可勾选可记录）
    - D 文件：磁盘保留（状态"回退删除"，只读；**记录时算删除**——record 前物理删除提交）
  - 回退前未提交改动暂存（`.keinestamp/rollback-backup/` + backup-meta.json，移出工作区）；**只备份首次回退**
  - **回退失败（首次）自动还原**：备份取回 + HEAD 若已被 branch reset 移动则还原回 head_before（防半状态）
  - 时间线灰显：**仅 X（superseded_point）灰显不可回退**；其余记录（含 X 之后、被顶替出主线的折叠记录）均可回退；回退任意记录会覆盖 marker（连续回退视为新回退）
  - **已检出 vs 已修改**：检出文件内容与检出时一致（marker `change_hashes` 判定）→ 标签「已检出」（不阻碍连续回退）；用户修改过（内容不符）→ 标签「已修改」；**untracked（不在树）不算树的修改**（`rollback_modified_files` 跳过——残留/副本/新建不进丢弃/恢复弹窗；丢弃时仍由 `discard_dirty` 物理删除）
  - **连续回退**（已有回退态再次回退）：**只备份首次回退**；判定"真修改"（`rollback_modified_files`：change_files 工作区 hash ≠ 检出 hash（marker `change_hashes`，检出时算 DefaultHasher），或非检出文件有改动）——仅真修改才弹「丢弃并跳转 / 留下」（`rollback` 带 discard 参数；未确认报"回退需要丢弃确认"）；**纯检出自动丢弃不弹窗**（只显示"已检出"不阻碍回退）；丢弃 = `discard_dirty`（M/D 还原 HEAD + **untracked 物理删除**（回退期间新建，首次回退时工作区 untracked 已被备份移出，不误伤）+ staged 报错）；**head_before 始终沿用首次回退**（恢复始终回到最初回退前，无中间态残留）；CLI 走「先 undo 再回退」自动恢复路径（main.rs cmd_rollback）
  - **首条记录（无父）回退特例**：新项目 init 时自动创建 **init 提交**（`.loreignore` only，metadata `kind=init`，GUI 时间线/log 过滤不显示——`init_commit` 幂等：有历史/无 .loreignore 跳过）→ **首条用户记录总有父（init）** → 回退首条 = `branch reset` 到 init（工作区只剩 .loreignore，用户文件检出为 **untracked**，changes 区可重新勾选/修改——初次提交可修改）；**旧项目（无 init 提交）首条回退**：`revision_parent` 无父 → reset 自身后**补创建 init 提交**（`force_init_commit`：物理删除树内用户文件 → stage D → commit（kind=init，父 = 首条，树 = 仅 .loreignore）→ `file write` 从首条写回 → 文件变 **untracked**；change_files = 写回列表（"已检出"、恢复不弹确认））——**新旧项目语义统一，无需重建**；恢复（undo）`branch reset` 回首条还原（补的 init 提交留作孤儿，--no-gc 保护不清理）。**None 分支要点**（实测踩坑）：① `repository dump` **必须带 `--revision`**（不带会受 `status --scan` 持久化的 untracked 标志污染——含已移出的文件，导致误删用户文件/写回 file not found）；② 物理删除：NotFound（磁盘无此树文件，用户改名/移动）视为已删（stage D 正常），其他错误（占用/权限）→ 明确报错并还原；③ 任一步失败 → `unstage_all` + `branch reset` 回 head_before（工作区完整恢复，不丢文件）
  - **dump 行解析**（2026-08-12 实测踩坑）：`repository dump` 行格式 `<path> id <N> ... addr ...`——**路径可能含空格**（`refer/芙姆米新1皮三视图 拷贝.png`、`ChatGPT Image 2026年7月24日 ...png`）→ **必须 `line.find(" id ")` 分隔取完整路径**（`split_whitespace().next()` 截断 → file write 报 `file not found: refer/...`——33_芙姆米 回退失败根因）；` addr ` 区分文件行（目录行无 addr 且路径带 `/`）——`dump_tree_files`/`export_files` 均已按此解析；`parse_status`/`parse_revision_changes` 用 `trim()`（完整路径，无此问题）
  - **撤销回退（恢复）** = `branch reset <head_before>` + 取回暂存；检出文件（change_files）默认丢弃；
    回退前暂存 ∩ 当前改动冲突 → 弹窗逐文件选择「当前版本改动 / 回退前改动」→ resolve 后恢复；
    **恢复确认**（2026-08-12）：changes 区有**真修改**（用户改过/删过内容）→ `undo_rollback(confirm: Option<bool>)`——None=报"恢复需要确认"、Some(true)=丢弃（修改与回退删除内容不取回）、Some(false)=留下（已修改检出取回）；**回退删除标记（deleted_files）属检出的一部分**（不触发确认，恢复自动处理）；纯检出直接执行；GUI 统一 pendingConfirm 弹窗「丢弃并恢复 / 留下」（空白区=取消不操作，弹窗显示修改文件列表）；**恢复失败（半状态）防御**：finish 失败且 HEAD 已移动 → 尽力还原回 rollback_point（marker 保留）；`get_rollback_state` 检测 marker 不一致（HEAD ≠ rollback_point，异常残留）→ 自动 clear_rollback
  - 顶替 = 回退状态下 record（新 revision 父 = X-1）；被顶替记录保留时间线（superseded 折叠灰显，时间序插入）
  - marker 存 timeline.json：rollback_point/superseded_point/head_before/deleted_files/change_files
  - `lore revision restore` 在 0.8.6 不可用（Invalid head）→ 弃用；`revert` = 撤销某 revision 改动（非撤销回退），冲突用 `resolve mine|theirs`
  - 点开头文件不可 stage（`Invalid path`）→ `.loreignore` 自动记录用 `lore --force stage`（实测可用）
- **Git safety 不适用**：v2 无 git；禁止在项目里建 git 仓库（集成测试除外）
- **MetaConfig** (`.keinestamp/.keinestamp.toml`)：`version`, `init_time`, `project_root`, `web_root`, `access_token`, `project_id`, `site_name`, `site_locked`（结构与 v1 相同；位于 `.keinestamp/` 私有目录，被 loreignore 排除，不参与提交）
- **AppConfig** (`%APPDATA%/Keinestamp/app-config.toml`)：`blender_path`, `deploy_token`, `deploy_server_url`, `auto_connect`, `close_to_tray`, `lore_server_url`
- **web 产物**：`.keinestamp/web/` 子目录（程序私有，loreignore 整体隔离），html_gen 输出 + timeline.json 缓存；**不入库**（commit_web 已删除）；zip 打包上传（push.rs）；web 版本号 = `.keinestamp/web-version` 本地计数器（`web_revision_count`/`bump_web_version`）
- **Atomic writes**: Temp file + rename (name includes PID + target filename)
- **init idempotent**: Re-init fills gaps, does not overwrite `init_time`
- **UNC paths**: Frontend `normalizePath()` strips `\\?\` prefix at entry. `open_folder`/`open_web_timeline` also strip before passing to OS commands
- **Export**: `lore repository dump` 解析文件树 + 逐文件 `lore file write`（不支持目录级 write）
- **Chinese filenames**: Lore 原生 UTF-8，无需 quotepath 处理
- **CLI output**: 中文标签（`已提交`、`已回退到记录 ...`、`已恢复回退前的状态`、`已推送到远端`）。Log 格式：`  {short_hash}  {date}  {author}  ({file_count} 文件)`
- **serve → loreserver**：serve.toml `enable_loreserver = true` 时启动探测 41339 health，离线则 spawn `loreserver --config <程序目录>/lore-server-config`（首次生成 local.toml：`[immutable_store.local].path` / `[mutable_store.local].path` → store 持久化于 `<程序目录>/lore-server-store`，重启数据保留）；在线则跳过；serve 退出时 kill 拉起的实例（外部已在线不杀）。启动后解析子进程输出打印结构化信息（`parse_loreserver_info`：版本 / store 路径 / auth 状态）+ 常驻读取线程转发后续输出（防管道阻塞）
- **备份**（2026-08-11 实测定稿）：GUI 备份 = `push_to_server`：读配置地址（app-config `lore_server_url`，默认 `lore://127.0.0.1:41337`）→ health 探测（`http://host:41339/health_check`）→ **仓库必须在线创建**（`lore repository create lore://host:port/<名>`，remote_url 自动绑定；**离线仓库 push 输出"Pushed"但数据不到服务器**，备份报"该项目为离线创建"）→ `lore push`（首推自动建分支，增量推送）。CLI `push` 同流程；`lore commit <MESSAGE>` 是位置参数（`-m` 不存在）；`lore repository config set` 不存在（remote_url 改 config.toml）。**loreserver 无认证（auth: None）**——仅限可信局域网，设置 UI 已标注
- **离线仓库迁移**（`migrate_offline_to_online`，实测）：离线仓库无法直接备份——`repository create <url>/<名> --id <同ID>` 预注册（空目录，服务器按 ID 绑定名字，**本地 store/hash/历史全保留**）→ 设 remote_url → push 全量；失败清理服务器残留。GUI：备份时检测离线 → 弹窗「迁移并备份」引导；校验：无回退态
- **3D 预览**：preview 路径存 timeline.json（不在 revision metadata）——rebuild_cache 从旧缓存沿用同 id 条目的 preview（记录后旧记录 3D 标签不丢）；`clean_old_glb_previews` **只删未被 timeline 引用的孤儿 GLB**（被引用文件即使超 max 也保留）
- **启动空态 + 远端仓库**（GUI）：**启动空态**（无项目打开时：一行提示 + 新建项目按钮，无卡片页）；**远端仓库管理在设置内**（SettingsDialog 第 4 Tab「远端仓库」，Remote.svelte `embedded` 紧凑模式，地址用已保存的 lore URL）：连接状态 + 自动刷新（进入页面拉取）+ 仓库列表（`list_server_repos` = `lore repository list` 解析 `name (id)`）+ **已下载行显示「跳转」按钮**（recent 目录名 == 仓库名 → 点击打开本地项目）替代下载勾选 + 未下载多选下载（`clone_server_repo` = `lore clone <url>/<name> <父目录>/<name>` + 重名检测 + `init_project` 补 .keinestamp 补洞 + add_recent）+ 删除（`delete_server_repo` = `lore repository delete <url>/<name>`，**弹窗输入 DELETE 确认**（输入自动转大写），删 = 备份数据丢失）；`localStorage.keinestamp_last_project` + AppConfig `auto_restore_project` 开关自动恢复上次项目；**部署连接独立于部署状态**（autoConn 时打开项目即探测服务器，未部署项目可首次部署）

## serve.rs

多租户 HTTP 服务器（web 部署）+ loreserver 拉起。结构与 v1 相同（两 token 认证、动态项目名路由、X-Project-Name、限速、路径穿越防护、100MB 上传限制），配置 `serve.toml`：

**程序目录基准**（`app_config::exe_dir()`，便携部署）：`KEINESTAMP_BASE_DIR` 环境变量优先（Linux 软链部署必须用它，`current_exe` 会解析到真实路径），否则 exe 所在目录。serve 命令查找 serve.toml：`--config` → `<程序目录>/serve.toml` → 命令参数目录（旧位置兼容）→ 自动生成 `<程序目录>/serve.toml`；**web_root 相对路径解析为相对程序目录**（绝对路径直接用）；`.keinestamp-projects.json` 在 web_root 父目录（web_root 默认 `<程序目录>/web-root` 时即程序目录）；loreserver 配置/store 在 `<程序目录>/lore-server-config|store`：

```toml
host = "0.0.0.0"
port = 7799
deploy_token = "hex-token"      # app → server upload/health auth
web_root = "web-root"           # 相对程序目录（便携）；也可绝对路径
enable_loreserver = true        # 可选：拉起 loreserver（备份服务器，持久化 store，跟随 serve 生命周期）
```

## detect.rs

| Condition | Result |
|---|---|
| Path not a dir | `Fresh` |
| No `.keinestamp.toml`, has `.blend` | `Existing` |
| Has `.keinestamp.toml` | `Initialized(MetaConfig)` |

Non-recursive `*.blend` scan. File path → error.

## lore-commands.md

`docs/lore-commands.md` — Lore v0.8.6 全部实测命令形态/输出格式/回退语义/去重率结论（Task 1 产物）。**任何新 lore 命令先补速查表再写代码。**

## Tauri GUI (src-tauri/)

`frontend/` — Svelte 5 + Vite + TypeScript (port 1420). `src-tauri/` — Tauri v2 crate wrapping lib.

```powershell
cargo tauri dev                   # Run dev (port 1420)
cargo tauri build                 # Production
cd frontend; npm install          # Install JS deps (three.js)
cd src-tauri; cargo build         # Backend only
```

### Tauri commands

全部在 `src-tauri/src/lib.rs`，lore 语义（v1 命令名保留，内部换后端）：

| Command | Returns | Notes |
|---|---|---|
| `detect` | `"fresh"\|"existing"\|"initialized"` | `.lore` 目录判断 |
| `init_project` | `MetaConfig` | lore 建仓 + 隐藏列表初始化 + add_recent |
| `record` | short hash (7) | `lore_project::record`；async emits `commit-progress`（5 阶段壳）；files 参数 = selected 提交；成功清回退标记 |
| `push` / `pull` | `()` | `lore push` / `lore sync` |
| `deploy` | `()` | web 部署（zip web/ 上传）；成功写 site_name + site_locked |
| `backup` | `()` | `lore push` 到本机/局域网 loreserver |
| `test_connection` / `get_deploy_status` / `has_remote` | 同 v1 | has_remote 检查 `.lore/config.toml` remote_url |
| `rollback` | `()` | branch reset + dirty 备份 + marker |
| `get_rollback_state` | `{rollback_point, head_matches}` | marker 读取；head_matches = current == rollback_point |
| `restore_rollback` | `()` | 撤销回退（branch reset 回 head_before） |
| `get_rollback_conflicts` / `keep_rollback_file` / `resolve_rollback_conflicts` | `[]` / `()` | v1 兼容签名，Lore 无 git 式冲突（UI 已移除） |
| `status` | `{clean, output}` | `changed_files`（过滤隐藏）；output 中文标签 `未提交的修改：` + `  {A/M/D/??} {path}` |
| `discard_file` | `()` | 取消单个文件更改（git restore）：`lore file reset <path>` 还原 HEAD——M 覆盖回 HEAD / D 从 HEAD 恢复（file reset 不建父目录，先 create_dir_all）；前端菜单仅 M/D 显示，M 弹确认框（2026-08-13） |
| `log` / `get_project_log` | `LogEntry[]` / `String[]` | revision_history 过滤 kind=web |
| `read_config` / `update_config` | 同 v1 | site_name 校验 ASCII + 锁定 |
| `get_timeline` | `Timeline` | 读 `web/timeline.json`（含 rollback_marker） |
| `export_entry` | `()` | dump + 逐文件 file write |
| `get_recent` / `add_recent` / `remove_recent` / `reorder_recent` | recent mgmt | |
| `open_folder` / `open_web_timeline` / `open_site` | `()` | 同 v1 |
| `get_ignore_info` | `{tracked: [], ignored, rule_ignored: []}` | ignored = 隐藏列表（hidden.rs load_hidden） |
| `add_to_gitignore` / `add_skip_worktree` / `remove_ignore` | `()` | 均为隐藏列表读写（add=添加路径，remove=移除；命令名保留） |
| `delete_entry` | `()` | timeline.json 删条目（不删 revision）+ 重建页面 + bump_web_version |
| `set_entry_milestone` | `()` | 写 timeline.json milestone 字段（revision metadata 回写不可行，实测 `metadata set --revision` 报错）+ 重建页面 + bump_web_version |
| `get_tracked_files` | `String[]` | walks dir（skips hidden/node_modules/target/web） |
| `get_file_diff` | `String` | `lore file diff --source <父> --target <rev>` |
| blender / settings / close_action 命令 | 同 v1 | |

### Frontend components

All in `frontend/src/`（组件在 `lib/`）。Key files:

- `App.svelte` — 主布局、状态、**10s 轮询**（`lore status` 实测 ~4s 是**在线仓库 + loreserver 离线时 remote 连接超时**——`run_lore` 统一强制 `--offline` 后本地操作全部毫秒级；push/sync/clone/list/delete/migrate create 等远程命令用 `run_lore_remote` 不带）；`status`/`get_rollback_state`/`get_project_log`/`check_lore_env` 均 **async + spawn_blocking**（同步命令会阻塞主线程导致 UI 冻结）；**run_lore 全局串行锁 + 60s 超时**（多进程并发开同一 store 会锁竞争/挂起，超时 kill 兜底）；**`revision_parent` 读 timeline 缓存**（`TimelineEntry.parent` 主线推导，零 lore 调用；旧缓存 fallback lore info）；`lore service` 0.8.6 实测不可用；GUI 回退/恢复**命令完成即清遮罩 + toast**（loadProject 后台刷新，避免"完成提示后转圈"）。恢复按钮（`rollback_point && head_matches` 时可用）；备份按钮接线；无冲突对话框（v2 已删 ConflictDialog）
- **store GC 语义**（2026-08-12 实测）：lore 分块 store 会清理「远端已有副本的本地孤儿块」——**在线仓库（push 过）** 的孤儿/历史块可被 `repository gc`/维护清理 → 回退被顶替记录报 `Address not found`（36_mine2 事故）；**离线仓库 GC 不删块**（本地唯一副本）；`run_lore` 统一带 **`--no-gc`** 阻止自动增量 GC（只读命令无副作用；显式 `repository gc` 仍可跑）；`Address not found` 由 `translate_lore_error` 转译为中文（"数据已不在本地存储：可能被清理或尚未从服务器下载"）；**clone 只下载最新状态** → `clone_repo_to` 后 `fetch_full_history`（逐 revision `repository dump` 在线拉全历史块，尽力而为）；**回退失败（首次）自动还原备份**（rollback_inner 失败 → `restore_backup_at`，避免 dirty 文件"消失"后下次回退清空备份目录丢数据）；**web 产物 dirty 卡 branch reset**：跳 web 父后 diff 含 web 提交改动 → 回退检出 `.keinestamp/web/` 文件（index.html/glb）→ 应用层 hidden 过滤看不到、lore 校验 tracked dirty 拒绝 reset → 修复：`revision_changes` 过滤 `.keinestamp/web/` 路径（不检出 web 产物）+ `reset_hidden_dirty`（branch reset 前从 HEAD 树 dump 取隐藏 tracked 清单 `file reset --force` 还原，loreignore 文件如 timeline.json status 不可见但 reset 仍校验，须 --force）接入 rollback_inner 与 finish_rollback_restore
- `lib/SettingsDialog.svelte` — 服务器/Blender 设置 + **隐藏路径管理**（列表 + 添加 + 删除，预设 web/、.keinestamp.toml 不可删）
- `lib/ActionBar.svelte` — 记录 / 恢复 / 备份 / 部署
- `lib/EntryList.svelte` — 时间线卡片（id = lore revision sig）、★ milestone、右键导出/回退、回退点灰显；**「文本改动」区块已删除**（2026-08-12：diff 展示依赖 file_status（后端恒空）导致 A 状态误显，直接移除——只需「文件变动」；`get_file_diff`/`file_diff` 后端命令同步删除）
- `lib/StatusPanel.svelte` — 变更面板（lore status 中文格式解析），勾选提交（files 参数），右键忽略/隐藏
- `lib/CommitDialog.svelte` — 记录弹窗（无回退警告，v2 顶替正常）
- `lib/statusParse.ts` — 解析 `未提交的修改：` + `  {code} {path}` 格式
- `api.ts` — Tauri invoke wrappers；`types.ts` — Interfaces（含 rollback_marker）
- 其余（TitleBar/ProjectTabs/Toast/BusyOverlay/DetailPanel/ConfirmDialog/InputDialog/CloseBehaviorDialog/StatusBar/Sidebar/DirTree）— 同 v1

**Dead code (don't use)**: ConflictDialog.svelte（已删除）；v1 的 git_project/git_helpers/git_web 已删除。

### Key design decisions

- **Config split**: Project-level MetaConfig vs app-level AppConfig（hidden_paths、deploy_token 等）
- **Token separation**: `deploy_token`（app 级，上传/健康）vs `access_token`（项目级，浏览器登录）；serve.rs 按端点区分
- **URL prefix routing**: `/<project_name>/` 动态路由，cookie 按 Path 隔离
- **Diff data externalized**: `web/diffs.json` loaded via fetch()
- **push/pull 门控**: `has_remote` 检查 config.toml remote_url（本机 loreserver 恒在 → 恒 true）
- **Notifications**: Toast auto-dismiss after 3s
- **Day/night dual theme**: 同 v1（人形/白泽，localStorage `keinestamp_theme` / `keinestamp_web_theme`）
- **Web page theme**: html_gen 模板双主题；登录页同主题；无 logo，data-URI favicon；根路径品牌落地页

## Integration tests

At `tests/cli_tests.rs`. Create real lore repos in `$env:TEMP/keinestamp_int_*`. Require loreserver 在线（127.0.0.1:41339）。Binary via `CARGO_BIN_EXE_keinestamp`。

## Test gaps (high-risk)

- `push.rs` — zip creation tested; `deploy_to_server` network path untested
- `main_helpers.rs` — 0 unit tests
- `serve.rs` — HTTP handlers + `ensure_loreserver` spawn 未单测（探测分支手工冒烟过）
- `lore_backend.rs` — revert/export_files 集成测试；loreserver 重启信任链实测通过未落测试
- `lore_project.rs` — undo_rollback 冲突路径（current != rollback_point 报错分支）测试

## Known dead code

- (none — v1 git 后端、ConflictDialog 已删除)
