# 服务端部署指南

`keinestamp serve` 提供两类能力：

1. **网页时间线部署**（多租户）：客户端把网页产物 zip 上传到服务器，浏览器通过 `/<项目名>/` 访问
2. **局域网备份服务器**（可选）：`enable_loreserver = true` 时拉起内置 `loreserver`，供 `备份` 使用

> 所有路径均为占位符：`<程序目录>` = serve 的数据目录；实际部署请替换为你的路径 / 域名 / 令牌，勿照抄示例。

---

## 1. 程序目录基准

serve 的配置与数据以**程序目录**为基准：

- 优先级：`KEINESTAMP_BASE_DIR` 环境变量 > exe 所在目录
- **软链部署必须设置 `KEINESTAMP_BASE_DIR`**：`current_exe()` 会把软链解析到真实路径
  （如 `源码目录/target/release/`），否则配置会散落到构建产物里，`cargo clean` 即丢失

```
<程序目录>/
├── keinestamp(.exe)          # 主程序（可为软链指向构建产物）
├── lore-bin/                 # Lore 运行时（构建时自动拷贝到 exe 同目录）
├── serve.toml                # 服务配置（首次 serve 可自动生成）
├── web-root/                 # 部署数据
│   ├── <项目名>/             # 每个项目的网页产物
│   ├── .versions.json        # 各站点版本计数
│   ├── .manifests/           # 各站点文件清单
│   └── _three/               # 共享 three.js 资源（启动时生成）
├── .keinestamp-projects.json # 项目令牌 / 身份注册表（web_root 的父目录）
├── lore-server-config/       # 仅 enable_loreserver = true 时生成
└── lore-server-store/        # loreserver 备份数据
```

---

## 2. serve.toml

查找顺序：`--config` 指定 > `<程序目录>/serve.toml` > 命令参数目录（旧位置兼容）> 自动生成。

```toml
host = "127.0.0.1"            # 默认只监听本机，对外走反代
port = 7799
deploy_token = "<随机长令牌>"  # 必填：上传 / 健康检查认证；为空直接拒绝启动
web_root = "web-root"         # 相对 = 相对程序目录；也可写绝对路径
trust_proxy = true            # 反代场景：信任 X-Forwarded-For / X-Forwarded-Proto
enable_loreserver = false     # 可选：拉起内置 loreserver（局域网备份）
site_name = "My Studio"       # 可选：网页首页标题（默认 Keinestamp）
```

> `deploy_token` 为空或纯空白会让 `Authorization: Bearer ` 直接通过认证，serve 会拒绝启动。

---

## 3. 启动方式

### Windows（手动 / 计划任务）

```powershell
.\keinestamp.exe serve . --port 7799
```

### Linux + systemd（推荐）

仓库自带 unit 模板 [deploy/keinestamp.service](../deploy/keinestamp.service)：

```bash
cp deploy/keinestamp.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now keinestamp.service
```

部署前请修改 unit 中的：

- `ExecStart` / `WorkingDirectory`：指向程序目录
- `Environment=KEINESTAMP_BASE_DIR=...`（软链部署必填）
- `ReadWritePaths`：**必须包含整个程序目录**（见下方坑 1）

---

## 4. 多租户与令牌

- 部署地址：`http://<host>:<port>/<项目名>/`，项目名即客户端设置的「网页名称」
- **`deploy_token`**：客户端上传 / 健康检查使用（`Authorization: Bearer <token>`）
- **项目 `access_token`**：浏览器登录使用；部署时客户端通过 `X-Access-Token` 注册到服务器
- 登录 Cookie 按 `/<项目名>/` 路径隔离，多个项目互不干扰
- 站点名在首次部署后**锁定**，作为访问路径；需要改名时请在服务器侧清理对应项目目录与注册表

---

## 5. 反向代理与 HTTPS

推荐链路：

```
浏览器 → Cloudflare(强制 HTTPS) → cloudflared 隧道 → Caddy(TLS) → serve(127.0.0.1:7799, 明文)
```

Caddy 示例：

```caddyfile
keinestamp.example.com {
    reverse_proxy 127.0.0.1:7799
}
```

要点：

- `serve.toml` 中 `trust_proxy = true`，否则拿不到真实客户端 IP 与来源协议
- Caddy 会按**入口协议**自动设置 `X-Forwarded-Proto`；
  **不要**手动 `header_up X-Forwarded-Proto https`，否则局域网 HTTP 入口也会被标成 https
- serve 对 HTTPS 来源的登录 Cookie 加 `Secure`，堵住 `http → https` 跳转前的明文盲区；
  局域网明文直连不受影响

---

## 6. 局域网备份服务器（loreserver）

```toml
enable_loreserver = true
```

- serve 启动时探测 `41339` 健康检查；不在线则拉起 `loreserver`
- 首次启动生成 `lore-server-config/local.toml`，store 持久化到 `lore-server-store/`
- 监听 `41337`（Lore gRPC/QUIC）+ `41339`（HTTP 健康检查）
- serve 退出时结束自己拉起的实例（外部已在线的不动）
- **loreserver 无认证（`auth: None`）——仅限可信局域网**

客户端（GUI）在 设置 → 服务器 填写 loreserver 的 IP / 端口（默认 `41337`），
点「备份」即可 `lore push`。离线仓库会引导「迁移并备份」。

---

## 7. 客户端配置

### GUI

设置 → 服务器 / 远端仓库：

- 服务器 IP / 端口（网页部署）
- loreserver IP / 端口（可勾选与部署共用 IP）
- 部署令牌（= 服务端 `deploy_token`）
- 开启服务开关后点「测试连接」

部署：时间线右侧设置「网页名称」「访问令牌」→ 底部「部署」。

### CLI

```powershell
# 启动服务
keinestamp serve <程序目录> --port 7799

# 部署项目
keinestamp deploy <项目目录> --server http://127.0.0.1:7799 --deploy-token <deploy_token>
```

---

## 8. 升级与备份

- 升级：停止服务 → 用新版本整目录替换（保留 `serve.toml`、`web-root/`、`.keinestamp-projects.json`、
  `lore-server-*`）→ 启动
- 备份：至少备份 `web-root/`、`.keinestamp-projects.json`、`serve.toml`；
  启用 loreserver 时再备份 `lore-server-store/`
- `.keinestamp-projects.json` 丢失会导致所有站点 token 失效（401），务必纳入备份

---

## 9. 安全建议

- `deploy_token` 使用随机长串；泄漏等于任何人都能上传/覆盖站点
- 公网部署请走 HTTPS（Caddy + Cloudflare 隧道），不要直接暴露明文 7799
- 项目访问令牌明文存储（服务器注册表 / 客户端项目配置），服务器安全依赖主机本身
- loreserver 无认证，不要暴露到公网

---

## 10. 故障排查

| 现象 | 原因 | 处理 |
|---|---|---|
| 启动报 `serve.toml 的 deploy_token 为空` | `deploy_token` 为空/纯空白（空 token = 无认证） | 填入随机长令牌后重启 |
| 上传 200 但 `.keinestamp-projects.json` 不存在 | systemd `ReadWritePaths` 未放开注册表目录 | 修正 unit → `daemon-reload` → `restart` → 重新部署 |
| 上传 403 `existing project has no identity` | 项目目录已存在但注册表无身份（防劫持） | 备份移走目录 → GUI 首次部署重新注册 |
| 登录 302 `login?error=1` | 用了 `deploy_token` 而非项目 `access_token` | 浏览器登录使用项目访问令牌 |
| 局域网 IP 无法访问 | 默认只监听 `127.0.0.1` | 改 `host = "0.0.0.0"` 或配置反代 |
| 公网 Cookie 未加 Secure | `trust_proxy` 未开启，或反代未透传协议 | `trust_proxy = true`，检查反代入口协议 |
| 重启后站点全部 401 | 注册表写盘失败或丢失 | 检查 `ReadWritePaths` 与备份 |

---

## 11. 相关文档

- [GUI 使用指南](gui-guide.md)
- [CLI 参考](cli.md)
- [Lore 0.8.6 命令速查（开发用）](lore-commands.md)
