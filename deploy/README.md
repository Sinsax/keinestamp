# Keinestamp serve 部署说明（Linux / systemd）

> 所有路径为占位符：`<程序目录>` = serve 的数据目录（见下）；`%h` = systemd 用户主目录。
> 实际部署时替换为你自己的路径/域名/令牌，勿照抄。

## 1. 程序目录基准（便携部署）

serve 的所有配置与数据以**程序目录**为基准：

- 优先级：`KEINESTAMP_BASE_DIR` 环境变量 > exe 所在目录（`current_exe()` 父目录）
- **软链部署必须设 `KEINESTAMP_BASE_DIR`**：`current_exe()` 会解析软链到真实路径
  （如 `源码目录/target/release/`），不设的话配置会散落到构建产物里，`cargo clean` 即丢失

目录结构：

```
<程序目录>/
├── keinestamp(.exe)          # 主程序（可为软链指向构建产物）
├── lore-bin/                 # lore 运行时（build.rs 自动拷贝到 exe 同目录）
├── serve.toml                # serve 配置
├── web-root/                 # 部署数据（serve.toml: web_root = "web-root"）
│   ├── <项目名>/             # 每个部署的项目一个目录
│   └── _three/               # 共享 three.js 资源（serve 启动自动生成）
├── .keinestamp-projects.json # 项目密钥注册表（tokens + ids）← web_root 父目录
├── lore-server-config/       # 仅 enable_loreserver = true 时
└── lore-server-store/        # loreserver 备份数据
```

## 2. serve.toml

查找顺序：`--config` 指定 > `<程序目录>/serve.toml` > 命令参数目录（旧位置兼容）> 自动生成 `<程序目录>/serve.toml`。

```toml
host = "127.0.0.1"            # 只监听本机，对外走反代（Caddy/nginx）
port = 7799
deploy_token = "<deploy_token>"
web_root = "web-root"         # 相对路径 = 相对程序目录；也可绝对路径
trust_proxy = true            # 反代场景：读 X-Forwarded-For / X-Forwarded-Proto
enable_loreserver = false     # 可选：拉起 loreserver（备份服务器）
```

## 3. systemd 部署（重要坑）

```bash
# 安装（把 deploy/keinestamp.service 放到用户服务目录）
cp deploy/keinestamp.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now keinestamp.service
```

**坑 1：注册表必须可写。** `.keinestamp-projects.json` 在 **web_root 的父目录**（不在 web-root 里面）。
如果 `ProtectHome=read-only` 且 `ReadWritePaths` 只放行 `web-root/`，注册表落盘会**静默失败**：
上传返回 200、内存注册可用（登录正常），但**重启 serve 后 token 全部丢失**。
→ `ReadWritePaths` 必须包含整个部署目录（示例模板已处理）。

**坑 2：必须设 `KEINESTAMP_BASE_DIR`**（软链部署时），否则程序目录解析到源码 target 目录。

## 4. 登录 cookie 动态 Secure（公网/局域网分流）

- serve 根据请求来源决定登录 cookie 是否带 `Secure`：
  - HTTPS 来源（`trust_proxy=true` 时 `X-Forwarded-Proto: https`，或 TLS 直连）→ 带 `Secure`
  - HTTP 直连（局域网自用）→ 不带（保持登录体验）
- 作用：堵住 `http://域名 → 301 → https` 跳转前首个明文请求携带 cookie 的盲区；
  局域网明文直连（自用场景）不受影响
- 反代要求：Caddy 按入口协议**自动**设置 `X-Forwarded-Proto`
  （https 入口 → https，http 入口 → http），serve 无需额外配置；
  **不要**手动 `header_up X-Forwarded-Proto https`（会把局域网入口也标成 https，破坏分流）
- 前置条件：`serve.toml` 必须 `trust_proxy = true`，且 serve 为含动态 Secure 的版本

## 5. 常见问题排查

| 现象 | 原因 | 处理 |
|---|---|---|
| 上传 200 但 `.keinestamp-projects.json` 不存在 | systemd `ReadWritePaths` 没放开注册表目录（见坑 1） | 修正 unit → `daemon-reload` → `restart` → 重新部署 |
| 上传 403 `existing project has no identity` | 项目目录已存在（手工拷贝）但注册表无身份（防劫持） | 备份移走目录 → GUI 首次部署重新注册 |
| 登录 302 `login?error=1` | 用了 deploy_token 而非项目 access_token | 浏览器登录用项目 access_token（注册表 tokens，GUI 部署时通过 `X-Access-Token` 头注册） |
| 局域网直连 8080 无法访问 | 反代只绑定了特定地址 | 反代绑定局域网 IP，或另配一个 http 入口 |
| 公网 `X-Forwarded-Proto` 不是 https | 反代未透传/入口是 http | Caddy https 入口自动正确；确认请求实际到达的入口协议 |

## 6. 公网安全建议（Caddy + Cloudflare）

- 推荐链路：浏览器 → Cloudflare(强制 HTTPS) → cloudflared 隧道 → Caddy(TLS) → serve(127.0.0.1:7799 明文)
- Caddy 作为 TLS 终端（`tls internal` 自签即可，仅隧道内部使用）
- 令牌明文存储（注册表 JSON / 客户端 MetaConfig）——服务器安全依赖主机本身
- loreserver 无认证（auth: None）——仅限可信局域网
