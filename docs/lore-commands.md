# Lore CLI v0.8.6 命令速查表（实测记录）

> Task 1 验证门产物。**所有命令参数与输出解析以本表为准**，禁止凭空猜测。
> 实测环境：`lore 0.8.6+373` / `loreserver 0.8.6`，安装于 `C:\Users\para\AppData\Local\lore\bin`。
> 验证脚本：`scripts/verify_lore.ps1`（四轮实测汇总为最终版）。

## 0. 架构事实（验证门最重要结论）

- **Lore 无纯本地（无服务器）模式**：所有操作依赖 `loreserver` 在线（`lore://host:port/<name>`）。Keinestamp v2 的本地使用 = 本机 spawn loreserver 子进程（同 Task 10 serve 集成方案，GUI 启动时也要拉起）。
- 无配置启动 loreserver = demo 模式：监听 41337（QUIC/gRPC）/ 41339（HTTP），store 在系统临时目录（`<temp>/lore-server`），auth 关闭，自签证书。
- 健康检查：`curl.exe http://127.0.0.1:41339/health_check` → `HTTP/1.1 200 OK`。
- 持久化服务器（生产 serve 集成）配置见 Task 10。

## 1. 通用规则

- 命令在仓库目录内运行（cwd 生效）；也可用全局 `lore --repository <path> <cmd>`（实测 `repository config get remote_url` 生效）。
- **revision 参数必须用签名（完整或前缀 hash），数字 N 不被接受**（报 `revision not found: N`）。
  数字 → 签名：`lore revision find number N`（输出纯签名一行）。
- 中文文件名原生 UTF-8 输出，无需 quotepath/转义处理。
- 错误输出走 stderr：`[Error] <msg>` + 源码位置行。

## 2. create / list / config

| 命令 | 实测输出 / 说明 |
|---|---|
| `lore repository create lore://127.0.0.1:41337/<name>` | `Created repository <name> in <path> with ID <32hex>`；在 cwd 建 `.lore/` |
| `lore repository list lore://host:port` | 每行 `<name> (<id>)` |
| `lore repository config get <key>` | 键为 config.toml 字段（实测 `remote_url` 可用→`lore://127.0.0.1:41337`）；未知键报 `[Error] Invalid repository path`（误导性报错，实际是键不存在） |
| config.toml 结构 | `remote_url`；`[store] max_capacity=2000000 eviction_delay=30 max_size=10737418240 compaction_delay=50`；`[file] direct_write/direct_io/flush_write` |

**忽略规则：无原生支持**（config get 无 ignore 键）→ 按计划走 GUI 隐藏列表方案（`web/` 预设隐藏）。

## 3. status（StatusPanel 依赖）

默认输出（无 `--scan` 不扫文件系统，只读持久化 dirty 标志）：

```
Repository <id>
On branch main revision N -> <sig>
Remote revision N -> <sig>
Local branch in sync with remote        # 或 Local branch is ahead of remote
```

`lore status --scan` 追加（实测格式）：

```
Untracked files:
A a.txt
A web/                  # 目录也会列出（`A <dir>/`）
A web/b.txt
Tracked changes: 3 added

Changes not staged for commit:
M a.txt
Tracked changes: 2 added, 1 modified

Changes staged for commit:
M a.txt (M)
D models/ (M)

Changes in conflict:
M  a.txt (M)!           # ! = 冲突
```

- 前缀：`A` 新增（含 untracked）、`M` 修改、`D` 删除；`(M)` 为元状态标记。
- `--scan` 的 dirty 标志刷新会持久化（后续 status/stage 无需重扫）。
- 其他选项：`--revision-only`（只显示 revision 头）、`--count`、`--check-dirty`、`--reset`。

## 4. stage / commit

| 命令 | 说明 |
|---|---|
| `lore stage <path>...` | 单文件对照文件系统；目录只 stage 已 dirty 文件；`--scan` 一次遍历发现+stage |
| `lore stage move <from> <to>` | 移动/重命名（保留文件身份） |
| `lore commit "<msg>"` | 提交 staged |

commit 输出（实测）：

```
Fragmenting files and updating tree hashes
Committing staged changes
Committed 1/1 directories, 2/2 files, 269.00 bytes/269.00 bytes (2 modified, 0 deleted)
Stored history for N nodes          # 仅非首次
Repository: <id>
Revision  : N
Signature : <64hex>
Parent    : <64hex>                 # 仅非首次
<metadata 行，如 `kind      : user`>   # 仅该 revision 有 metadata 时，位于 Parent 后
Branch    : <branch sig>
Date      : Mon, 10 Aug 2026 11:57:09 +0000
    <message>
Commit succeeded
```

- 有冲突时拒绝：`[Error] Unable to commit when <file> is still in conflict`。
- 空 stage（无实际变化）也能提交成功（实测 10 次相同内容提交均成功）→ **业务层需先判断 changed 非空**。

## 5. metadata（D4 语义数据权威）

| 命令 | 说明 |
|---|---|
| `lore revision metadata set <key> <value>` | 写 **staged state** 的 metadata；可多次 set 多个键；输出 `kind      : user` 确认行 |
| `lore revision metadata get [key]` | 默认当前 revision；`--revision <sig>` 指定；带 key 只输出该键 |
| `lore revision metadata clear` | 清 staged metadata；输出 `Metadata cleared for revision <sig>` |

- 流程：`stage` → `metadata set kind user` → `commit`（commit 时写入该 revision，commit 输出展示）。
- metadata 附着 staged state：set 一次后不 clear 会延续到后续提交（T1.5 实测第二次 commit 仍带 `kind : user`）。
- revert 自动写 `reverted-from: <sig>`。

## 6. history / revision info

```
lore history [LENGTH]               # 每 revision 块（Revision/Signature/Branch/Date/message）
lore history [LENGTH] --oneline     # 每行 `N <message>`，N = revision number，最新在前
lore revision info <sig>            # Revision/Signature/Parent/Branch/Date/message
lore revision find number N         # 输出签名一行
```

- 被 branch reset 顶替出主线的 revision 仍可通过 find number / sig 访问（数据保留在 store）。

## 7. 回退语义（Task 4 依赖，实测定稿）

> **`lore revision restore` 在 0.8.6 不可用**（任何形态均报 `[Error] Invalid head`，restore.rs:139）→ 弃用。
> 回退模型改用 `branch reset` + `revert`（如下）。

### 回退到记录（对应 v1 rollback）

```
lore branch reset <记录sig>
# 输出：Calculating deltas N -> M / Verifying X changes with local file system / Reset branch main to <sig>
```

- 要求：**无 staged state、无 dirty 文件**，否则拒绝（见下方保护）。
- 效果：分支 latest 指针 + 工作区文件同步还原到该 revision；`history` 只剩回退点之前的 revision（顶替语义基础）。

### 撤销回退（对应 v1 restore_rollback）

```
lore branch reset <回退前 HEAD 的 sig>
```

- 回退前先记录 HEAD sig（timeline 缓存字段，如 `rollback_marker = {rollback_point, head_before}`）。
- 实测：回退→commit 顶替→branch reset 回原 sig，history 完整恢复（c1 c2 c3），工作区还原。**完全可逆**。
- head_matches 判定：`history` 顶部 revision sig 是否 = 记录 sig（或对比缓存字段）。

### 顶替（回退状态下记录）

```
回退（branch reset 到记录）→ 直接 stage + commit
```

- 新 revision 的 parent = 回退点 sig；被顶替 revision 从主线 history 消失（数据仍在 store）。
- 实测：history `2 c4 supersede / 1 c1`（原 c2/c3 消失）。

### revert（撤销某 revision 的改动，非"撤销回退"）

```
lore revision revert <sig>
# 输出：Starting revert of revision <sig>
#       Calculating 3-way diff between / base N -> <sig> / source 0 -> 0000... / target N -> <cur>
#       干净：Merged files, X updated, Y deleted, Z merged, W conflicted / Staged reverted repository state <sig>
#       冲突：Files in conflict: <file>...
```

- 产生新 revision（不顶替），冲突/删除自动 staged；metadata 自动写 `reverted-from`。
- 冲突解决：`lore revision revert resolve mine|theirs <path>...`（或 `abort`/`restart`），全部解决后 `lore commit`。

### 保护行为（dirty / staged）

| 状态 | 错误 |
|---|---|
| dirty 文件 | `[Error] File has local changes: <file> (incoming size X bytes, file system size Y bytes)` / `Local modifications prevent synchronization` |
| staged state | `[Error] Unable to reset branch when there is a staged state` / `Unable to sync when there is a staged state` |

→ lore_project 回退流程：有 staged 先 `lore unstage`；有 dirty 文件需**备份复制到临时目录 → branch reset → 恢复备份**（Blender 打开文件场景，Task 4 实现）。

## 8. file write / diff / dump（导出，Task 5 依赖）

| 命令 | 说明 |
|---|---|
| `lore file write --path <file> --revision <sig> --output <dest>` | 单文件导出，输出 `File written <path>`；`--path` 必填 |
| `lore repository dump [--revision <sig>] [--path <p>] [--max-depth N]` | 文件树列表（v1 git ls-tree 替代） |
| `lore file diff [paths]... [--source <sig>] [--target <sig>]` | unified diff：`--- <file>@N` / `+++ <file>` |
| `lore revision diff <src> [--target <dst>] [--path <p>]` | revision 间 diff |

dump 输出格式（实测）：

```
Repository <id>
Revision <sig>
Revision  : N
Signature : <sig>
Tree: hash <64hex> size N
a.txt id 1 parent 0 sibling 0 mode 00 size 7 flags 1 addr <addr>
models/ id 2 parent 0 sibling 1 mode 00 size 11 flags 0 child 3
models/m.blend id 3 parent 2 sibling 0 mode 00 size 11 flags 1 addr <addr>
```

- **`file write` 不支持目录路径**（报 `file not found: <dir>`）→ 目录级导出 = dump 解析文件列表 + 逐文件 write（v1 `git ls-tree` + `git show` 同构）。

## 9. push / clone / sync（Task 10 备份链路）

| 命令 | 实测输出 |
|---|---|
| `lore push` | `Pushing N fragment(s)` / `Pushed N fragment(s), X bytes` / `Pushing <sig> to branch main` / `Pushed revision N -> <sig> to branch main`（每个本地 revision 一条） |
| `lore clone lore://host:port/<name> <dir>` | `Cloning repository <id> branch main into <path>` / `Pull state <sig>` / `Cloned N/N files (X bytes/Y bytes)` / `Branch main revision <sig>` / `Clone complete in Xs` |
| `lore sync [<sig>]` | 同步到远端/指定 revision；要求无 staged state |

## 10. 去重率（止损验证，用户已实测 + 复现）

- **6MB 随机二进制 × 10 次提交（每次改末尾 2KB）→ `.lore` 增长 81,165 bytes / 63MB 全量 = 去重率 99.9%**（止损线 ≥50%，远超）。
- 用户实测结论：未压缩 .blend 方案采用（.lore 增长慢、记录时间略长）；压缩 .blend 否决（连续压缩流级联失效、移动模型去重失效）。

## 11. 中文文件名

- 全链路通过：status 显示 `A 角色模型.blend`、commit/history/info 原生 UTF-8，无需处理。

## 12. 对计划的影响（Task 2+ 调整点）

1. Task 2 `commit()`：stage → metadata set（先 clear）→ commit；revision id 用 find number 或 commit 输出解析。
2. Task 4 回退：`restore` 弃用 → `branch reset`（回退/撤销回退/顶替），revert 仅作"撤销改动"；回退前 dirty 备份方案。
3. `run_lore` 封装：所有命令需 `--non-interactive` 谨慎（未实测交互场景，0.8.6 默认非交互）。
4. 本地使用必须保证 loreserver 在线（GUI spawn 子进程，同 Task 10）。
