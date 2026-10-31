# CLI 参考

`keinestamp` 同时是 GUI 程序与命令行工具。CLI 与 GUI 共用同一套后端逻辑与数据格式。

> 除 `download-blender` / `list-blender-versions` 外，所有命令都会先校验 Lore 版本必须为 `0.8.6`。
> 本地命令内部强制 `--offline`（毫秒级）；只有 `push` / `pull` / `deploy` 等远程命令才会联网。

```
keinestamp <COMMAND> [ARGS]
```

路径参数均可省略，默认当前目录（`.`）。

---

## init

初始化项目（创建 Lore 仓库、`.keinestamp/`、`.loreignore`）。可重复执行，只补缺不覆盖。

```powershell
keinestamp init <path>
```

| 参数 | 说明 |
|---|---|
| `path` | 项目目录，默认 `.` |

## record（别名 commit）

记录一次进度。提交**全部已 stage 的勾选文件**（CLI 下为工作区全部变更），可附带说明。

```powershell
keinestamp record <path> -m "说明"
```

| 参数 | 说明 |
|---|---|
| `path` | 项目目录 |
| `-m, --message <msg>` | 记录说明（必填） |

> GUI 用于「勾选提交」；CLI 直接记录工作区全部变更（隐藏 / 忽略规则仍然生效）。

## status

查看未提交变更（中文标签，过滤隐藏规则）。

```powershell
keinestamp status <path>
```

## log

查看时间线（最新在前）。

```powershell
keinestamp log <path> -n 20
```

| 参数 | 说明 |
|---|---|
| `-n, --count <N>` | 显示条数，默认 20 |

输出格式：`  {短ID}  {日期}  {作者}  ({文件数} 文件)` + 下一条说明。

## rollback

回退到指定记录的**父版本**，并把该记录的改动检出为未提交修改。

```powershell
keinestamp rollback <path> -e <记录ID>
```

| 参数 | 说明 |
|---|---|
| `-e, --entry <id>` | 时间线记录 ID（完整 hash 或前缀） |

> 已有回退状态时先自动执行 `restore` 再回退。

## restore

撤销回退，恢复到回退前的状态（工作区 + HEAD）。

```powershell
keinestamp restore <path>
```

## export

把某条记录的文件版本导出到目录（逐文件 `lore file write`）。

```powershell
keinestamp export <path> -e <记录ID> -o <输出目录>
```

| 参数 | 说明 |
|---|---|
| `-e, --entry <id>` | 记录 ID |
| `-o, --output <dir>` | 输出目录（不存在会自动创建） |

## push / pull

备份 / 同步到远端（loreserver 或任意 Lore 服务器）。

```powershell
keinestamp push <path>     # 备份：lore push
keinestamp pull <path>     # 同步：lore sync
```

要求：仓库为**在线创建**（已绑定 `remote_url`）。离线仓库请先迁移（GUI 备份会引导），
或使用 `lore repository create <url>/<名> --id <同ID>` 预注册后推送。

## config

查看 / 修改项目配置与全局提交身份。

```powershell
keinestamp config <path>                      # 查看
keinestamp config <path> --set access_token=<token>
keinestamp config <path> --set identity=para@local
keinestamp config <path> --set auto_connect=true
```

| key | 作用域 | 说明 |
|---|---|---|
| `access_token` | 项目 | 网页访问令牌（浏览器登录用） |
| `identity` | 全局 | 提交身份（写入 `lore --identity`），空值回落 `keinestamp@local` |
| `auto_connect` | 全局 | 打开项目时自动测试连接 |

## serve

启动网页服务（多租户部署 + 静态托管），可同时拉起内置 loreserver。

```powershell
keinestamp serve <path> -p 7799 --host 0.0.0.0
```

| 参数 | 说明 |
|---|---|
| `-p, --port <PORT>` | 监听端口，默认 `7799` |
| `--host <HOST>` | 绑定地址，默认 `0.0.0.0` |
| `--config <FILE>` | 指定 `serve.toml`（默认为程序目录下自动生成） |

初始化时若 `serve.toml` 不存在会自动生成。部署、登录、令牌与 loreserver 细节见
[服务端部署指南](server-guide.md)。

## deploy

把当前项目的网页产物部署到 `serve`（zip 上传）。

```powershell
keinestamp deploy <path> --server http://127.0.0.1:7799 --deploy-token <token>
```

| 参数 | 说明 |
|---|---|
| `--server <url>` | 服务器地址（必填） |
| `--deploy-token <token>` | 上传认证令牌（服务器 `serve.toml` 中的 `deploy_token`） |

## Blender 版本

```powershell
keinestamp list-blender-versions
keinestamp download-blender -v 4.2.0
```

| 参数 | 说明 |
|---|---|
| `-v, --version <ver>` | 指定版本；省略则交互选择 |

> 这两个命令不依赖 Lore。

---

## 退出码与输出

- 成功：输出中文结果（如 `已提交`、`已回退到记录 ...`），退出码 0
- 失败：`错误：...` 输出到 stderr，退出码非 0
- Lore 相关错误会转译为中文（如 `Address not found` → 数据不在本地存储）
