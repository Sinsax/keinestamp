# CLI reference

**English** | [简体中文](zh-CN/cli.md)

`keinestamp` is both the GUI application and a command-line tool. The CLI and the GUI share the
same backend logic and data format.

> Except for `download-blender` / `list-blender-versions`, every command first verifies that the
> Lore version is `0.8.6`. Local commands force `--offline` internally (millisecond latency);
> only remote commands such as `push` / `pull` / `deploy` go to the network.

```
keinestamp <COMMAND> [ARGS]
```

Path arguments may be omitted and default to the current directory (`.`).

---

## init

Initializes a project (creates the Lore repository, `.keinestamp/` and `.loreignore`). Safe to run
repeatedly: missing pieces are filled in, existing ones are never overwritten.

```powershell
keinestamp init <path>
```

| Argument | Description |
|---|---|
| `path` | Project directory, defaults to `.` |

## record (alias: commit)

Records progress. Commits **all staged files** (in the CLI: every change in the workspace) with an
optional message.

```powershell
keinestamp record <path> -m "message"
```

| Argument | Description |
|---|---|
| `path` | Project directory |
| `-m, --message <msg>` | Record message (required) |

> The GUI is for “commit the ticked files”; the CLI records everything changed in the workspace
> (hidden / ignore rules still apply).

## status

Shows uncommitted changes (localized labels, hidden rules filtered out).

```powershell
keinestamp status <path>
```

## log

Shows the timeline (newest first).

```powershell
keinestamp log <path> -n 20
```

| Argument | Description |
|---|---|
| `-n, --count <N>` | Number of entries, defaults to 20 |

Output format: `  {short-id}  {date}  {author}  ({file count} files)` followed by the message.

## rollback

Rolls back to the **parent version** of the given record and checks that record's changes out as
uncommitted modifications.

```powershell
keinestamp rollback <path> -e <record-id>
```

| Argument | Description |
|---|---|
| `-e, --entry <id>` | Timeline record ID (full hash or prefix) |

> If a rollback state already exists, `restore` runs automatically first, then the rollback.

## restore

Undoes the rollback and returns to the state before it (workspace + HEAD).

```powershell
keinestamp restore <path>
```

## export

Exports the file versions of a record into a directory (per-file `lore file write`).

```powershell
keinestamp export <path> -e <record-id> -o <output-dir>
```

| Argument | Description |
|---|---|
| `-e, --entry <id>` | Record ID |
| `-o, --output <dir>` | Output directory (created if missing) |

## push / pull

Backs up / syncs to a remote (a loreserver or any Lore server).

```powershell
keinestamp push <path>     # backup: lore push
keinestamp pull <path>     # sync: lore sync
```

Requirement: the repository must have been **created online** (with a bound `remote_url`). For
offline repositories, migrate first (the GUI backup flow will guide you), or pre-register with
`lore repository create <url>/<name> --id <same-id>` and then push.

## config

Shows / changes project configuration and the global commit identity.

```powershell
keinestamp config <path>                      # show
keinestamp config <path> --set access_token=<token>
keinestamp config <path> --set identity=you@local
keinestamp config <path> --set auto_connect=true
```

| key | Scope | Description |
|---|---|---|
| `access_token` | Project | Web access token (for browser login) |
| `identity` | Global | Commit identity (passed as `lore --identity`); empty falls back to `keinestamp@local` |
| `auto_connect` | Global | Test the connection automatically when opening a project |

## serve

Starts the web service (multi-tenant deployment + static hosting) and can spawn the bundled
loreserver.

```powershell
keinestamp serve <path> -p 7799 --host 0.0.0.0
```

| Argument | Description |
|---|---|
| `-p, --port <PORT>` | Listen port, defaults to `7799` |
| `--host <HOST>` | Bind address, defaults to `0.0.0.0` |
| `--config <FILE>` | Use a specific `serve.toml` (by default one is generated in the program directory) |

If `serve.toml` does not exist, it is generated on first run. Deployment, login, tokens and
loreserver details are covered in the
[server deployment guide](server-guide.md).

## deploy

Deploys the current project's web output to `serve` (zip upload).

```powershell
keinestamp deploy <path> --server http://127.0.0.1:7799 --deploy-token <token>
```

| Argument | Description |
|---|---|
| `--server <url>` | Server address (required) |
| `--deploy-token <token>` | Upload credential (the `deploy_token` from the server's `serve.toml`) |

## Blender versions

```powershell
keinestamp list-blender-versions
keinestamp download-blender -v 4.2.0
```

| Argument | Description |
|---|---|
| `-v, --version <ver>` | Target version; omitted means interactive selection |

> These two commands do not require Lore.

---

## Exit codes and output

- Success: localized result (e.g. `已提交` in Chinese, `Committed` in English), exit code 0
- Failure: `Error: ...` (`错误：...` in Chinese) on stderr, non-zero exit code
- **Language**: follows the system or the app setting (Settings → General → Language); scripts can
  force it with `KEINESTAMP_LOCALE=zh-CN` / `en-US`
- Lore errors are translated with hints (e.g. `Address not found` → data no longer in local store)
- The `Usage:` lines and argument-error skeletons generated by clap stay in English (library-fixed
  text)
