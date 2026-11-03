# i18n catalog

全仓**唯一**的多语言文案真源。前端（`frontend/src/lib/i18n`）与 Rust（`src/i18n.rs`）
读取同一份文件，不要复制到别处。

## 文件

| 文件 | 说明 |
|---|---|
| `zh-CN.json` | 简体中文（默认语言） |
| `en-US.json` | English |

两份文件的键集**必须完全相同**（`cargo test` 强制校验），唯一例外见 `web.` 前缀。

## 键命名

`surface.area.item`，嵌套 JSON 表达，查找用点号路径。

| surface | 用途 | 谁使用 |
|---|---|---|
| `common` | 通用词（保存/取消/关闭/文件数…） | 前端 + 后端 |
| `gui` | 桌面界面文案 | `frontend/src` |
| `cli` | 命令行输出与 help | `src/main.rs` |
| `err` | 错误消息 | Rust 各模块 |
| `web` | 生成的网页 / 服务器页面 | `html_gen.rs`、`serve.rs` |

## 约定

- **占位符**：`{name}`，运行时替换。数字用 `{n}`。
- **复数**：写成 `key.one` / `key.other` 两个叶子。英文按 `n == 1` 选 `one`；
  中文两句写同样的文本（无复数变化），调用方统一用 `tn(key, n)` / `tf`。
- **`web.` 例外**：网页端按产品决策**恒为英文**，因此 `web.*` 只存在于 `en-US.json`。
  这两点由 `src/i18n.rs` 的一致性测试保证。
- **不翻译**：项目名、流程名、标签名、文件名、提交信息、Lore/Blender 原始输出。
- 文件必须 UTF-8、无 BOM。

## 夹具

新增 key 的流程：先加 `zh-CN.json`，再补 `en-US.json`（漏了 `cargo test` 会失败）；
Rust 里用 `i18n::t("gui.x.y")`，前端用 `t('gui.x.y')`（key 类型由 zh-CN.json 推导，拼错即编译失败）。
