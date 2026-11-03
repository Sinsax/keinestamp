# Server deployment guide

**English** | [简体中文](zh-CN/server-guide.md)

`keinestamp serve` provides two capabilities:

1. **Web timeline deployment** (multi-tenant): the client uploads a zip of the web output to the
   server and browsers access it at `/<project-name>/`
2. **LAN backup server** (optional): with `enable_loreserver = true` a bundled `loreserver` is
   started for `Backup`

> All paths below are placeholders: `<program-dir>` is serve's data directory. Replace them with
> your own paths / domain / tokens; do not copy the examples verbatim.

---

## 1. Program directory base

serve's configuration and data are relative to the **program directory**:

- Priority: `KEINESTAMP_BASE_DIR` environment variable > the directory containing the exe
- **Symlink deployments must set `KEINESTAMP_BASE_DIR`**: `current_exe()` resolves symlinks to
  the real path (e.g. `source-dir/target/release/`), otherwise configuration scatters into build
  artifacts and is lost on `cargo clean`

```
<program-dir>/
├── keinestamp(.exe)          # main program (may be a symlink to build output)
├── lore-bin/                 # Lore runtime (copied next to the exe at build time)
├── serve.toml                # service config (auto-generated on first serve)
├── web-root/                 # deployment data
│   ├── <project-name>/       # web output per project
│   ├── .versions.json        # version counter per site
│   ├── .manifests/           # file manifest per site
│   └── _three/               # shared three.js assets (generated at startup)
├── .keinestamp-projects.json # project token / identity registry (parent of web_root)
├── lore-server-config/       # generated only when enable_loreserver = true
└── lore-server-store/        # loreserver backup data
```

---

## 2. serve.toml

Lookup order: `--config` > `<program-dir>/serve.toml` > command argument directory (legacy
location) > auto-generate.

```toml
host = "127.0.0.1"            # listen on localhost by default; expose via a reverse proxy
port = 7799
deploy_token = "<random-long-token>"  # required: upload / health-check auth; empty refuses to start
web_root = "web-root"         # relative = relative to the program dir; absolute paths allowed
trust_proxy = true            # behind a reverse proxy: trust X-Forwarded-For / X-Forwarded-Proto
enable_loreserver = false     # optional: spawn the bundled loreserver (LAN backup)
site_name = "My Studio"       # optional: landing page title (default Keinestamp)
```

> An empty or whitespace-only `deploy_token` would let `Authorization: Bearer ` authenticate
> blindly, so serve refuses to start.

---

## 3. Starting

### Windows (manual / scheduled task)

```powershell
.\keinestamp.exe serve . --port 7799
```

### Linux + systemd (recommended)

The repository ships a unit template:
[deploy/keinestamp.service](../deploy/keinestamp.service)

```bash
cp deploy/keinestamp.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now keinestamp.service
```

Before deploying, edit the unit:

- `ExecStart` / `WorkingDirectory`: point at the program directory
- `Environment=KEINESTAMP_BASE_DIR=...` (required for symlink deployments)
- `ReadWritePaths`: **must include the whole program directory** (see pitfall 1 below)

---

## 4. Multi-tenancy and tokens

- Deploy URL: `http://<host>:<port>/<project-name>/`, where the project name is the “site name”
  configured in the client
- **`deploy_token`**: used by the client for upload / health checks
  (`Authorization: Bearer <token>`)
- **Project `access_token`**: used for browser login; the client registers it with the server via
  `X-Access-Token` during deployment
- Login cookies are isolated per `/<project-name>/`, so multiple projects do not interfere
- The site name is **locked** after the first deployment (it is the URL path); to rename it, clean
  up the project directory and the registry on the server

---

## 5. Reverse proxy and HTTPS

Recommended chain:

```
browser → Cloudflare (force HTTPS) → cloudflared tunnel → Caddy (TLS) → serve (127.0.0.1:7799, plain)
```

Caddy example:

```caddyfile
keinestamp.example.com {
    reverse_proxy 127.0.0.1:7799
}
```

Key points:

- Set `trust_proxy = true` in `serve.toml`, otherwise the real client IP and origin scheme are lost
- Caddy sets `X-Forwarded-Proto` automatically from the **incoming** protocol;
  **do not** add `header_up X-Forwarded-Proto https` manually, or plain LAN HTTP entries would be
  marked as https too
- serve marks login cookies from HTTPS origins as `Secure`, closing the plaintext gap during the
  `http → https` redirect; plain LAN access is unaffected

---

## 6. LAN backup server (loreserver)

```toml
enable_loreserver = true
```

- On startup serve probes the `41339` health endpoint; if offline it spawns `loreserver`
- The first start generates `lore-server-config/local.toml`; the store persists under
  `lore-server-store/`
- Listens on `41337` (Lore gRPC/QUIC) + `41339` (HTTP health check)
- serve terminates instances it spawned on exit (external instances are left alone)
- **loreserver has no authentication (`auth: None`) — trusted LAN only**

In the client (GUI) enter the loreserver IP / port in Settings → Server (default `41337`) and click
“Backup” to `lore push`. Offline repositories are guided through “Migrate and back up”.

---

## 7. Client configuration

### GUI

Settings → Server / Remote repositories:

- Server IP / port (web deployment)
- loreserver IP / port (can optionally share the deployment IP)
- Deploy token (= the server's `deploy_token`)
- Turn on the service switches and click “Test connection”

Deploy: set the “site name” and “access token” on the right of the timeline → click “Deploy” in the
bottom bar.

### CLI

```powershell
# start the service
keinestamp serve <program-dir> --port 7799

# deploy a project
keinestamp deploy <project-dir> --server http://127.0.0.1:7799 --deploy-token <deploy_token>
```

---

## 8. Upgrading and backup

- Upgrading: stop the service → replace the whole directory with the new version (keep
  `serve.toml`, `web-root/`, `.keinestamp-projects.json`, `lore-server-*`) → start
- Backup: at minimum back up `web-root/`, `.keinestamp-projects.json` and `serve.toml`; when
  loreserver is enabled also back up `lore-server-store/`
- Losing `.keinestamp-projects.json` invalidates every site token (401) — always include it in
  backups

---

## 9. Security recommendations

- Use a long random `deploy_token`; leaking it means anyone can upload / overwrite sites
- For public deployments use HTTPS (Caddy + Cloudflare tunnel); never expose plaintext 7799 directly
- Project access tokens are stored in plaintext (server registry / client project config); server
  security depends on the host itself
- loreserver has no authentication — never expose it to the internet

---

## 10. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Startup: `serve.toml deploy_token is empty` | `deploy_token` empty or whitespace (an empty token means no auth) | Set a long random token and restart |
| Upload returns 200 but `.keinestamp-projects.json` is missing | systemd `ReadWritePaths` does not allow the registry directory | Fix the unit → `daemon-reload` → `restart` → deploy again |
| Upload 403 `existing project has no identity` | The project directory exists but the registry has no identity (anti-hijack) | Move the directory away → deploy again from the GUI to re-register |
| Login 302 `login?error=1` | Used `deploy_token` instead of the project `access_token` | Use the project access token for browser login |
| LAN IP unreachable | Only `127.0.0.1` is bound by default | Set `host = "0.0.0.0"` or configure a reverse proxy |
| Public cookie missing Secure | `trust_proxy` off, or the proxy does not forward the scheme | Set `trust_proxy = true` and check the proxy's incoming scheme |
| All sites return 401 after a restart | Registry write failed or was lost | Check `ReadWritePaths` and your backups |

---

## 11. UI language

- **Web output is always English**: the deployment page, login page and root landing page are all
  English (including empty states and errors), `<html lang="en">`. This is a product decision —
  web pages face external visitors and do not follow the server's language setting
- **Server console output** follows the UI language (same as the CLI: `KEINESTAMP_LOCALE` → config
  → system language)
- **User data is not translated**: project names, process names, tag names, file names and commit
  messages are shown verbatim

## 12. Related documentation

- [GUI guide](gui-guide.md)
- [CLI reference](cli.md)
- [Lore 0.8.6 command cheat sheet (development, Chinese)](lore-commands.md)
