# 多用户协作：跟随 Lore 语义（v2）

> 状态：**待确认** · 日期：2026-11-02 · 代码基线：Keinestamp v0.5.1（Lore 0.8.6）
> 本版**取代** v1 的「team.json 入库 + 自造租约 + 自造冲突 UI」方案。
> 依据：Lore 官方设计文档（epicgames.github.io/lore，system-design / server-config）+ 本机
> `loreserver` 6 轮实测（两个克隆、分叉、冲突、锁、metadata、保护分支）。

## 1. 结论

Lore 的多人协作已经是一套完整语义：

**集中式远端 + 乐观并发（CAS push）+ 自动 merge + 分支保护 + 二进制文件锁 + JWT 权限 + 可变 metadata**

Keinestamp 不需要发明协作机制，需要的是**把应用流程对齐到这套语义**。
v1 方案里自造的「团队状态文件 / 自造租约 / 自造冲突解决」都不必要——Lore 原生命令已经覆盖，
只有 UI、离线缓存、状态展示需要自己写。

## 2. Lore 的多人机制（官方语义 + 本机实测）

| # | 机制 | Lore 的设计 | 本机实测（0.8.6，两个克隆） |
|---|---|---|---|
| A | **远端是唯一真源** | 分支 latest、权限、锁、metadata 都在服务端 mutable store；客户端本地可 commit/切换分支，push/sync/lock/metadata 需要在线 | 本地操作全部离线可用；lock/metadata 离网即失败 |
| B | **乐观并发 + CAS** | push 时若远端 latest 已前进 → 拒绝；`sync` 拉取并做三方合并，生成**双 parent 的 merge revision**，再 push | `[Error] Branch has diverged, sync to merge remote changes` → `lore sync` 自动 `performing merge` → 生成 `3 Merge main into main` → 再 push 成功 |
| C | **冲突 = pending merge** | 同文件双改时 merge 停在冲突态，逐文件 `branch merge resolve mine|theirs`，commit 后 push | `Changes in conflict: M scene.blend (M)!`；resolve + commit + push 全链路通过 |
| D | **二进制用锁，不用合并** | `lock acquire/release/status`，服务端记录，其他客户端可查 | 跨客户端可见（`scene.blend by <unknown>`）；B 再 acquire 被拒（`Lock already owned`） |
| E | **权限 = JWT** | `[server.auth]` 开启 JWT 校验（jwt_issuer / jwt_audience / JWKS），按 partition 授权；**未配置时所有人拥有全部权限** | 未配置时：B 能 release A 的锁、能给被锁文件 push —— 锁退化为提示 |
| F | **共享可变状态** | `repository metadata` / `branch metadata`：服务端可变存储，不进历史、不需要 push | A 写入 `team.board` 后 B **立刻**读到；离线读 → `Address not found` |
| G | **分支是一等公民** | name→latest 存在 mutable store，创建免费；`branch protect` 阻止直接 push | 保护后 B push 报 `Not authorized to access repository` |
| H | **事件订阅** | `notification subscribe [seconds]` 订阅仓库事件 | 未验证 |

上游原文（system-design §13.4）：*"A locked file cannot be modified by anyone other than the lock
holder; an attempt by another client to push a change to a locked file is rejected by the server."*
——前提是服务端知道「谁是谁」，即 E 必须开启。

## 3. 实测原文（关键摘录）

~~~text
# 分叉
B> lore push
[Error] Branch has diverged, sync to merge remote changes

# 同步即自动合并
B> lore sync
Remote and local branch have diverged, performing merge
Starting merge of branch ... revision 4f70...
B> lore push
Pushed revision 3 -> aa5142... to branch main

# 同文件冲突
B> lore status
Local branch has diverged, synchronize to merge
Pending merge, incoming revision 857619...
Changes in conflict:
M  scene.blend (M)!
B> lore branch merge resolve theirs scene.blend
B> lore commit "merge: take A scene"   # 合并提交
B> lore push                          # 成功

# 共享可变状态（A 写、B 读，无需 push）
A> lore repository metadata set team.board '{"milestones":["abc123"]}'
B> lore repository metadata get team.board
team.board: {"milestones":["abc123"]}

# 锁
A> lore lock acquire scene.blend
B> lore lock status scene.blend
Files locked for edit:
scene.blend by <unknown> on Mon, 2 Nov 2026 14:27:04 +0000
B> lore lock acquire scene.blend
Lock already owned on files: scene.blend
B> lore lock release scene.blend     # 无 auth 时：能释放别人的锁（实测）
Lock released on files: scene.blend
~~~

## 4. 对齐到 Keinestamp

### 4.1 记录 = commit → push → 分叉就 sync → 再 push

Lore 的协作协议只有这一条，直接照搬：

1. 本地 `commit`（现有 `record`）。
2. 在线仓库 → 立即 `push`。
3. push 报 `Branch has diverged` → 自动 `sync`（Lore 自动合并，无需人工）→ 再 `push`。
4. sync 进入冲突态 → UI 展示冲突文件，逐文件「采用远端 / 保留本地」→ `branch merge resolve` → `commit` → `push`。
5. 解析 `lore status` 的 `Local branch has diverged` / `Pending merge` / `Changes in conflict` 做全局状态。

这一步做完，「两个人先后记录」就自动成立，文本与非重叠改动**零人工**。

### 4.2 看板状态 = `repository metadata`（在线）+ 本地缓存

- 里程碑、删除墓碑、标签模板写 `keinestamp.board.*`（一个 JSON 值即可）。
- 不需要额外 revision、不改 `.loreignore`、不产生历史噪声。
- 离线读不到 → 本地缓存上次值，UI 标注「离线（同步于 …）」；写入必须在线。
- 服务端无 per-key 权限，注意写入方校验（客户端 + 可选 serve 层校验）。

### 4.3 编辑锁 = `lore lock`

- 打开 `.blend/.psd` 或点「记录」前 `lock acquire`；关闭/提交后 `release`。
- 展示持有者：`lock status <path>`。
- **无 auth 时只是软锁**（实测可被他人释放、不阻止 push）→ UI 必须写清楚，或者先做 4.5。

### 4.4 身份 = `lore auth`（OIDC）或 `--identity`（仅展示）

- 有 `[server.auth]` 时：`lore login --token`，服务端把 token 的 identity 记为 Creator/Committer，锁与保护才真正生效。
- 没有时：现有 `--identity` 只能展示，不能作为安全边界（现状）。

### 4.5 命令必须分「离线 / 在线」两类

当前 `run_lore` 一刀切 `--offline`（`src/lore_backend.rs`），这是接入 Lore 多人的**最大阻碍**：
`push / sync / lock / metadata / status --remote` 都需要在线通道。需要按命令白名单区分，
并给网络命令单独的短超时与失败降级。

### 4.6 分支（可选）

- 小团队：单 main + sync 合并即可（4.1）。
- 发布线：`branch protect main` 阻止直接推送；但无 auth 时保护会连正常用户一起拒绝（实测），
  所以「保护」要和 4.5 的 auth 一起上。

## 5. 与 v1 方案的差异

| 主题 | v1（自造） | v2（跟随 Lore） |
|---|---|---|
| 团队看板状态 | 提交 `.keinestamp/team.json`（改 loreignore 白名单、产生 revision、要合并 JSON） | `repository metadata`，零提交、即时共享 |
| 二进制互斥 | 自造租约表 + 服务端 API | `lore lock` |
| 冲突处理 | 自建冲突 UI / 手工 | `sync` 自动 merge + `branch merge resolve` |
| 身份 | AppConfig 单值 + 服务端用户表 | `lore auth`（OIDC JWT）；无 auth 时保持展示性 |
| 同步 | 自建 pull + 备份/还原 | `push → diverged → sync → push` 闭环 |

## 6. 分阶段

| 阶段 | 内容 | 依赖 |
|---|---|---|
| P0 | `run_lore` 分在线/离线；在线 `status` 解析分叉/待合并/冲突；状态栏展示 | 无 |
| P1 | record 自动 push + 分叉自动 sync + 冲突态 UI（mine/theirs 按来源标注） | P0 |
| P2 | 看板状态迁 `repository metadata` + 本地缓存 + 离线标注 | P0 |
| P3 | `lore lock` 接入（打开/记录前 acquire，展示持有者，关闭 release） | P0 |
| P4 | auth 决策：自建 OIDC（Dex/Keycloak）+ `[server.auth]`，锁与保护变强制 | 运维 |
| P5 | `notification subscribe` 实时事件、分支保护、审计 | P1 |

## 7. 待验证与风险

| 项 | 状态 | 影响 |
|---|---|---|
| `lore lock` 是否有 TTL / 自动过期 | **未验证** | 崩溃后锁残留，需要手动清理入口 |
| 无 auth 时锁不可强制（可被他人 release、不阻止 push） | **已实测** | 产品文案不能承诺「锁死」 |
| `repository metadata` 离线不可读 | **已实测** | 看板需本地缓存 + 离线标注 |
| `resolve mine/theirs` 方向与 git 相反（实测 mine=对方内容、theirs=本地内容） | **已实测** | UI 必须按「远端/本地」标注，不能照抄 git 文案 |
| loreserver 被强杀会丢最近写入（team4 强杀后远端分支消失；`flush_delay_seconds=1` 并等待后正常） | **已实测** | `serve.rs` 的 `child.kill()` 需改成优雅退出，否则「备份完立刻关机」可能丢最后一次 push |
| `[server.auth]` 需要外部 OIDC 提供方 | 文档确认 | 小团队落地成本高，需产品决策 |
| 冲突文件是二进制时 merge 的可用性 | 部分验证（.blend 冲突可 resolve，不做内容合并） | 说明「Lore 不合并二进制」，锁才是正解 |

## 8. 命令速查（本机实测）

~~~text
lore push                      # 分叉时报 Branch has diverged
lore sync                      # 自动 merge，生成双 parent revision
lore status                    # 含 Remote revision / Pending merge / Changes in conflict
lore branch merge resolve mine|theirs <path>
lore lock acquire|status|release <path>
lore repository metadata set|get|clear [key] [value]
lore branch protect|unprotect <branch>
lore notification subscribe [seconds]
lore branch create|switch|merge|archive
lore repository create <url>/<name> / clone / list / delete
~~~

## 9. 身份与设备绑定（增补，2026-11-02 晚）

### 9.1 为什么「user@host + MAC」不能当身份

| 问题 | 说明 |
|---|---|
| MAC 不稳定 | Windows 11 Wi-Fi 默认开「随机硬件地址」，每连一个网络换一个；换网卡/扩展坞/开虚拟机/重装系统都会变 → 用户会被反复当成新人 |
| 主机名不稳定 | 可随手改、重装即变、跨组织重名常见 |
| **可伪造** | 两个值都是客户端自己报的，改一个字符串就能冒充别人；这不是安全边界 |
| 概念错位 | 人 ≠ 设备：一人两台机器、两人一台机器都会算错；身份主键应该是账号，设备只是「哪台机器」的标签 |
| 隐私 | 采集 MAC 属于设备标识符，越来越敏感 |

更根本的一条：**「锁定硬件」的前提是服务器能验证，而客户端自报的硬件信息天生无法验证。**

### 9.2 推荐：账号 + 服务端签发的设备凭证

要的「两个都对上才算一个人」可以做到，但两个因子改成**可验证**的：

1. 用户账号（用户名 + 密码/邀请码）。
2. **设备凭证**：首次在某台机器登录时，服务器生成随机 32 字节密钥（不可猜），
   客户端存进系统凭据管理器（Windows Credential Manager / DPAPI），**不用 MAC**。
3. 每次取 token 时，服务器校验「账号 + 密码 + 设备凭证 + 设备未被吊销」，三者都对才签发。
4. 换机器 = 重新登记一台设备（可设「需管理员批准」），而不是被识别成另一个人；
   丢电脑 = 服务端吊销该设备。
5. 主机名 / MAC / 系统 MachineGuid 只作为**设备标签**显示和审计，不做判据。

这与「改了前者就算另外一个用户」的差别：**改设备标签不会产生新身份，换账号才会。**
真正想要「一台机器一把钥匙」时，设备凭证就是那把钥匙，而且能吊销、不可伪造。

### 9.3 落地方式：serve 当最小 JWT 签发方（已实测门禁可开）

Lore 服务端的认证只认 JWT：`[server.auth]` + `jwt_issuer` / `jwt_audience` / JWKS。
本机实测：起一个 30 行的 JWKS 服务（Node 生成 RSA 密钥、暴露 `/jwks.json`），
把 loreserver 指过去后——

~~~text
lore repository create lore://127.0.0.1:41337/authrepo
[Error] creating repository on server: code: 'The request does not have valid authentication credentials',
        message: "authorization header required"
~~~

也就是说：**门禁开关是确定存在的、由我们自己的签发方控制**。
Keinestamp 的 `serve` 本来就在常驻，天然适合做这个签发方：

- 用户表 / 设备表放在 serve（或 serve 的 SQLite）。
- 签发短期 JWT（`sub` = 用户名，Lore 用 `sub`/identity claim 认人）。
- lore 服务端配置 `[server.auth]` 指向 serve 的 JWKS。
- 之后推送的 Creator、锁的持有者、分支保护才真正可归属。

**待验证**（下一步 spike）：`lore login --token --token-type` 如何把我们的 JWT 交给 CLI；
以及带 token 后 push 是否被识别为用户、锁是否真的拒绝非持有者推送。
（文档 §13.4 承诺会拒绝，但需实测确认。）

### 9.4 占坑（锁）要怎么用

Lore 的锁是命令，**本身不会自动**；「要不要手动」是应用层决定。建议三档：

| 方式 | 触发 | 体验 |
|---|---|---|
| 半自动（推荐先做） | 右键「占用编辑」或双击 .blend 打开时自动 acquire | 一次点击，不用记命令 |
| 自动释放 | 记录完成 / 关闭文件 / 空闲 30 分钟 | 防止占着不用 |
| 兜底 | 记录时发现文件没锁 → 自动补占再提交 | 保证「改了就占」 |

限制与必备设计：

- **锁没有 TTL**（`lock acquire --help` 无过期参数，实测）→ 崩溃会留死锁，
  必须显示「张三 10:32 起占用」，并提供「申请释放 / 管理员强制释放」。
- 只有在 9.3 的门禁开启后，「只能自己改」才成立；不开门禁时任何人都能改、能拔锁（已实测）。
- 自动占坑容易「只看也占」，建议只在真正开始编辑（写操作）时占。

## 10. 账号密码 = 工牌（决策）

- 不再做设备绑定；账号即身份，令牌即工牌。
- 实现要点：密码用 argon2/bcrypt 哈希；serve 持有用户表；登录换短期 JWT（`sub` = 用户名）；
  首次启动生成管理员；管理员可重置密码/停用账号；审计记录「谁在何时 push/deploy/rollback」。
- 需要「只允许登记设备」时再叠加白名单（默认不开），而不是默认绑 MAC。

## 11. 平台化方向（Gitea 式）与预览

**结论：能兼顾，而且现在就应该把预览按「服务端派生数据」来设计。**

要区分两种预览（对应两种用途）：

| | 工程视图（项目内） | 发布视图（展示用） |
|---|---|---|
| 用途 | 团队看每个 revision 改了什么 | 对外展示、策展 |
| 绑定 | (仓库, revision, 文件路径) | 一次发布（deploy） |
| 生成 | 服务端按需渲染（Blender 无头） | 复用工程预览，挑选后打包 |
| 存放 | 服务端缓存，键 = 源文件 hash + 管线版本 | web_root/<项目>/ |
| 是否入库 | **否**（派生数据，可重建） | 否 |

关键点：预览是派生数据，永远可重建，所以**不进 Lore 历史**——这正好符合 Lore 的
「immutable store 只存被提交的内容」语义。平台化只是把生成位置从「每台机器本地」
换成「服务端一次、处处复用」。

Gitea 式能力与 Lore 原语的映射：

| 平台能力 | Lore 原语 |
|---|---|
| 仓库列表 / 组织 | partition（JWT 的 resource grant） |
| 权限（读/写/管理） | token 的 per-partition 权限集 |
| PR / 合并请求 | branch + `branch protect` + `branch merge` |
| 评审意见 | revision metadata（不可改）+ 应用层表 |
| issue / 看板 | `repository metadata` 或 serve 侧 DB |
| webhook / 事件 | `notification subscribe` |
| 多仓库组合 | links（入库）/ layers（本地叠加） |

## 12. 值得参考的协作模型（只取能落到 Lore 原语上的）

| 参考 | 核心机制 | 映射到 Keinestamp |
|---|---|---|
| **Perforce Helix** | 独占签出（exclusive checkout）、changelist、label | 二进制默认锁；一次记录 = 一个 changelist；里程碑 = label |
| **Plastic / Unity VC 的 Gluon** | 美术只同步自己要改的文件，锁驱动工作区 | Lore `.lore/view` 稀疏视图 + 懒加载 + 锁 → 「美术模式」 |
| **GitHub / GitLab** | 分支保护、CODEOWNERS、PR 评审 | `branch protect` + metadata 存 owners + 任务分支合并当评审 |
| **Gitea / GitLab 组织** | org / team / repo / role / 审计 | serve 的用户、项目成员、角色、审计日志 |
| **影视/游戏资产管线（ShotGrid、P4）** | work / publish 分离，v001/v002 版本号 | 工作文件 vs 发布产物分离（= §11 的两种预览） |
| **Perforce Swarm** | shelve 暂存给同事看 + review | Lore 无 shelve；用「草稿分支 + metadata 评审状态」近似 |
| **SVN** | 目录级权限 | Lore 只有仓库级 partition；目录权限只能在应用层做，别硬造 |

不建议照搬：Git 的 rebase/多分支 PR 复杂流（Lore 是 merge revision 模型）、CRDT 实时协同、目录级 ACL。
