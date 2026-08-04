# GLB 加密实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 部署到服务器的 GLB 预览文件以密文（AES-256-GCM）形式存在，浏览器端解密渲染，"防君子"档位，服务器零改动。

**Architecture:** 加密点选在 `blender_render::export_to_glb` 导出成功后（唯一咽喉，CLI/Tauri 两条路径自动覆盖）。导出后立即用随机密钥加密 `.glb` → `.glb.enc` 并删除明文，密钥写入 `previews_glb/keys.json`；`html_gen.rs` 生成 index.html 时把 `.enc` URL 与 key/iv 内嵌进 `glbData`；浏览器查看器改为 fetch → WebCrypto 解密 → `GLTFLoader.parse`。`serve.rs`/`push.rs` 零改动（已核实：serve.rs:785 未知扩展名有 `application/octet-stream` 兜底；zip 全目录递归自动携带 .enc 与 keys.json）。

**Tech Stack:** Rust（`aes-gcm`、`rand`、`base64` 三个新依赖）、WebCrypto（浏览器端）、现有 Svelte/three.js 不变。

## Global Constraints

- 定位"防君子"：不做水印、不做服务端解密、不做按访客授权、不做密钥轮换命令（轮换 = 删 previews_glb 让内置 Blender 按需重导）
- 加密点必须位于 `export_to_glb` 内部（成功后立即执行），保证 CLI/Tauri/get_glb_preview 三路径全覆盖，不得在调用方重复实现
- 明文 `.glb` 加密后必须立即删除；`web-timeline/` git 仓库、部署 zip、服务器任何环节都不允许再出现明文 GLB
- 向后兼容：keys.json 无对应条目的文件（旧明文 .glb）回退明文 URL
- 每文件独立随机 32B key + 12B IV，AES-256-GCM（自带认证）
- 密钥清单格式固定：`{"<glb_name>": {"key": "<base64>", "iv": "<base64>"}}`
- 验证命令：`cargo test`（lib + integration）+ `cd frontend; npm run check`
- 现有测试基线：67 unit + 7 integration（AGENTS.md 写的 70 是陈旧文档值，非回归）
- 根目录 `package-lock.json`（SakiStep 残留）永不提交
- 提交消息格式：`feat(glb): ...`

---

### Task 1: `src/glb_crypto.rs` 新模块 + 依赖

**Files:**
- Create: `src/glb_crypto.rs`
- Modify: `Cargo.toml`（新增 `aes-gcm`、`rand`、`base64`）
- Modify: `src/lib.rs`（挂载 `pub mod glb_crypto;`，按现有模块声明风格）

**Interfaces:**
- Produces（后续任务依赖的精确签名）：
  ```rust
  pub fn generate_key_iv() -> (String, String)                    // (key_b64, iv_b64)
  pub fn encrypt(data: &[u8], key_b64: &str, iv_b64: &str) -> Result<Vec<u8>>
  pub fn decrypt(data: &[u8], key_b64: &str, iv_b64: &str) -> Result<Vec<u8>>
  #[derive(Clone, Serialize, Deserialize)] pub struct GlbKeyInfo { pub key: String, pub iv: String }
  pub type KeyManifest = BTreeMap<String, GlbKeyInfo>;
  pub fn load_manifest(dir: &Path) -> Result<KeyManifest>          // 缺失返回空 map，不报错
  pub fn save_manifest(dir: &Path, manifest: &KeyManifest) -> Result<()>  // 原子写（temp + rename，沿用项目惯例）
  pub fn encrypt_glb(glb_path: &Path) -> Result<PathBuf>          // 读→随机 key/iv→加密→写 .enc（删除原 .glb 后返回 .enc 路径）；不写 manifest
  pub fn update_manifest(dir: &Path, glb_name: &str, info: &GlbKeyInfo) -> Result<()>  // load→upsert→save
  pub fn prune_manifest(dir: &Path) -> Result<()>                 // 删除 .enc 已不存在的条目
  ```

- [ ] **Step 1: 添加依赖到 `Cargo.toml`**

```toml
aes-gcm = "0.10"
rand = "0.8"
base64 = "0.22"
```
（追加在现有 `zip = "2"` 之后。依赖版本以 cargo 能解析为准，若 `cargo test` 编译报版本冲突，选兼容版本。）

- [ ] **Step 2: 创建 `src/glb_crypto.rs`（完整实现）**

```rust
use anyhow::{Context, Result};
use aes_gcm::{aead::{Aead, KeyInit, OsRng}, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlbKeyInfo {
    pub key: String,
    pub iv: String,
}

pub type KeyManifest = BTreeMap<String, GlbKeyInfo>;

pub fn generate_key_iv() -> (String, String) {
    let mut key = [0u8; 32];
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut key);
    rand::thread_rng().fill_bytes(&mut iv);
    (B64.encode(key), B64.encode(iv))
}

pub fn encrypt(data: &[u8], key_b64: &str, iv_b64: &str) -> Result<Vec<u8>> {
    let key = B64.decode(key_b64).context("invalid key base64")?;
    let iv = B64.decode(iv_b64).context("invalid iv base64")?;
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let nonce = Nonce::from_slice(&iv);
    cipher.encrypt(nonce, data).context("AES-GCM encrypt failed")
}

pub fn decrypt(data: &[u8], key_b64: &str, iv_b64: &str) -> Result<Vec<u8>> {
    let key = B64.decode(key_b64).context("invalid key base64")?;
    let iv = B64.decode(iv_b64).context("invalid iv base64")?;
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let nonce = Nonce::from_slice(&iv);
    cipher.decrypt(nonce, data).context("AES-GCM decrypt failed (auth tag mismatch)")
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("keys.json")
}

pub fn load_manifest(dir: &Path) -> Result<KeyManifest> {
    let path = manifest_path(dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save_manifest(dir: &Path, manifest: &KeyManifest) -> Result<()> {
    let path = manifest_path(dir);
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).context("failed to create manifest dir")?;
    let tmp = parent.join(format!(
        "keys.json.tmp.{}.{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
    ));
    let json = serde_json::to_string_pretty(manifest)?;
    {
        let mut f = std::fs::File::create(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;
        f.write_all(json.as_bytes())?;
    }
    std::fs::rename(&tmp, &path).with_context(|| format!("failed to rename {} -> {}", tmp.display(), path.display()))
}

pub fn encrypt_glb(glb_path: &Path) -> Result<PathBuf> {
    let data = std::fs::read(glb_path).with_context(|| format!("failed to read {}", glb_path.display()))?;
    let (key_b64, iv_b64) = generate_key_iv();
    let ciphertext = encrypt(&data, &key_b64, &iv_b64)?;
    let enc_path = glb_path.with_extension("glb.enc");
    std::fs::write(&enc_path, &ciphertext).with_context(|| format!("failed to write {}", enc_path.display()))?;
    std::fs::remove_file(glb_path).with_context(|| format!("failed to remove plaintext {}", glb_path.display()))?;
    Ok(enc_path)
}

pub fn update_manifest(dir: &Path, glb_name: &str, info: &GlbKeyInfo) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    manifest.insert(glb_name.to_string(), info.clone());
    save_manifest(dir, &manifest)
}

pub fn prune_manifest(dir: &Path) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let before = manifest.len();
    manifest.retain(|name, _| dir.join(format!("{}.glb.enc", name)).exists());
    if manifest.len() != before {
        save_manifest(dir, &manifest)?;
    }
    Ok(())
}
```

- [ ] **Step 3: `src/lib.rs` 挂载模块**（在现有 `pub mod` 声明区追加 `pub mod glb_crypto;`）

- [ ] **Step 4: 写单测（追加到 `src/glb_crypto.rs` 文件末尾）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("keinestamp_glb_crypto_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let (key, iv) = generate_key_iv();
        let data = b"hello glb \x00\x01\x02\xff";
        let ct = encrypt(data, &key, &iv).unwrap();
        assert_ne!(&ct, data);
        let pt = decrypt(&ct, &key, &iv).unwrap();
        assert_eq!(&pt, data);
    }

    #[test]
    fn wrong_key_fails() {
        let (key, iv) = generate_key_iv();
        let (key2, _) = generate_key_iv();
        let ct = encrypt(b"secret model", &key, &iv).unwrap();
        assert!(decrypt(&ct, &key2, &iv).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let (key, iv) = generate_key_iv();
        let mut ct = encrypt(b"secret model", &key, &iv).unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0xff;
        assert!(decrypt(&ct, &key, &iv).is_err());
    }

    #[test]
    fn encrypt_glb_writes_enc_and_deletes_plaintext() {
        let dir = temp_dir("enc_glb");
        let glb = dir.join("abc_hello.glb");
        std::fs::write(&glb, b"GLB fake content").unwrap();
        let enc = encrypt_glb(&glb).unwrap();
        assert!(enc.ends_with(".glb.enc"));
        assert!(!glb.exists());
        assert!(enc.exists());
        let (key, iv) = generate_key_iv();
        update_manifest(&dir, "abc_hello", &GlbKeyInfo { key, iv }).unwrap();
        let m = load_manifest(&dir).unwrap();
        assert!(m.contains_key("abc_hello"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn manifest_load_missing_returns_empty() {
        let dir = temp_dir("no_manifest");
        assert!(load_manifest(&dir).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prune_removes_missing_entries() {
        let dir = temp_dir("prune");
        let m: KeyManifest = [
            ("keep".to_string(), GlbKeyInfo { key: "a".into(), iv: "b".into() }),
            ("gone".to_string(), GlbKeyInfo { key: "c".into(), iv: "d".into() }),
        ].into_iter().collect();
        save_manifest(&dir, &m).unwrap();
        std::fs::write(dir.join("keep.glb.enc"), b"x").unwrap();
        prune_manifest(&dir).unwrap();
        let after = load_manifest(&dir).unwrap();
        assert!(after.contains_key("keep"));
        assert!(!after.contains_key("gone"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```
（若 `rand 0.8` 的 `fill_bytes` 用法与所选版本有 API 差异，以编译错误为准调整；`use std::io::Write` 若未用到则删除。）

- [ ] **Step 5: 运行测试验证**

Run: `cargo test --lib glb_crypto`
Expected: 6 个测试全过。再跑 `cargo test --lib` 确认既有单测无回归。

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/glb_crypto.rs
git commit -m "feat(glb): AES-256-GCM 加密模块与密钥清单"
```

---

### Task 2: 接入导出管线（blender_render.rs）

**Files:**
- Modify: `src/blender_render.rs`
- Test: `src/blender_render.rs`（追加单测）

**Interfaces:**
- Consumes: Task 1 的 `encrypt_glb`、`update_manifest`、`prune_manifest`、`GlbKeyInfo`
- Produces:
  - `export_to_glb` 成功导出后立即加密并返回 `.enc` 路径；"已存在则跳过"判断改查 `.glb.enc`
  - `clean_old_glb_previews(dir, max)` 同时清理 `.glb` 与 `.glb.enc`，随后 `prune_manifest`

- [ ] **Step 1: `export_to_glb` 改动（当前逻辑见 blender_render.rs:77-155）**

- "跳过"检查（~line 83）改为：
```rust
    let enc_output = output_path.with_extension("glb.enc");
    if enc_output.exists() {
        eprintln!("[debug] export_to_glb: GLB already exists, skipping: {:?}", enc_output);
        return Ok(enc_output);
    }
```
- 成功分支（`Blender exited 0 and output exists` 处，~line 136）在确认 `output_path.exists()` 后追加：
```rust
        let enc_path = crate::glb_crypto::encrypt_glb(&output_path)
            .with_context(|| format!("GLB encrypt failed for {:?}", output_path))?;
        let glb_name = output_name.to_string();
        let (key, iv) = crate::glb_crypto::generate_key_iv();
        let info = crate::glb_crypto::GlbKeyInfo { key, iv };
        crate::glb_crypto::update_manifest(
            output_dir,
            &glb_name,
            &info,
        )?;
        eprintln!("[debug] export_to_glb: encrypted → {:?}", enc_path);
        return Ok(enc_path);
```
注意：update_manifest 前必须把同一次加密用的 key/iv 写入 —— 需调整实现顺序：先 `generate_key_iv`，再手动 encrypt 并写 `.enc`，避免 `encrypt_glb` 内部生成密钥后无法取回。**推荐实现**：本任务内改为显式两步（不调用 `encrypt_glb`，而是 `let data = fs::read; let ct = glb_crypto::encrypt(&data, &key, &iv); fs::write(enc, ct); fs::remove_file(output)`），这样密钥与 manifest 一致。若保持调用 `encrypt_glb`，则必须接受其不返回密钥的事实并改用下列替代方案：调用 `encrypt_glb` 后 `load_manifest` 无法得知新密钥 —— 因此**必须显式两步**，如上。
- 返回类型仍为 `Result<PathBuf>`，现在返回 `.enc` 路径。

- [ ] **Step 2: `clean_old_glb_previews` 改动（当前 ~line 188-211）**

- 过滤条件从 `ext == "glb"` 改为 `ext == "glb" || ext == "enc"`（`.glb.enc` 的 extension 是 `enc`）
- 排序后保留最新 `max` 个（现有逻辑不变）
- 删除列表执行后追加：
```rust
    let _ = crate::glb_crypto::prune_manifest(dir);
```

- [ ] **Step 3: 单测（追加到 `src/blender_render.rs`）**

```rust
#[cfg(test)]
mod glb_encrypt_tests {
    use super::*;
    use crate::glb_crypto;

    #[test]
    fn clean_removes_glb_and_enc_and_prunes_manifest() {
        let dir = std::env::temp_dir().join(format!("keinestamp_clean_enc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(dir.join(format!("{}.glb", name)), b"x").unwrap();
            std::fs::write(dir.join(format!("{}.glb.enc", name)), b"y").unwrap();
        }
        let m: glb_crypto::KeyManifest = [
            ("a".to_string(), glb_crypto::GlbKeyInfo { key: "k".into(), iv: "v".into() }),
            ("c".to_string(), glb_crypto::GlbKeyInfo { key: "k".into(), iv: "v".into() }),
        ].into_iter().collect();
        glb_crypto::save_manifest(&dir, &m).unwrap();
        // max = 1：应保留最新的一个 .glb 与一个 .glb.enc（按文件名排序取最大）
        clean_old_glb_previews(&dir, 1).unwrap();
        let glbs: Vec<_> = std::fs::read_dir(&dir).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |x| x == "glb" || x == "enc"))
            .collect();
        assert_eq!(glbs.len(), 2, "应各保留 1 个 .glb 与 .glb.enc");
        let manifest = glb_crypto::load_manifest(&dir).unwrap();
        assert_eq!(manifest.len(), 1, "修剪后只剩保留条目的密钥");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```
（若 `clean_old_glb_previews` 现有签名/排序逻辑与假设不符，以实际代码为准调整断言 —— 核心断言：.glb 与 .glb.enc 成对保留 max 个、manifest 被修剪。）

- [ ] **Step 4: 运行测试验证**

Run: `cargo test --lib blender_render`
Expected: 新增测试通过；既有测试无回归。再跑 `cargo test` 全量确认。

- [ ] **Step 5: Commit**

```bash
git add src/blender_render.rs
git commit -m "feat(glb): 导出管线接入加密（导出即加密、清理含 enc、manifest 修剪）"
```

---

### Task 3: 页面侧嵌入（html_gen.rs）

**Files:**
- Modify: `src/html_gen.rs`
- Test: `tests/cli_tests.rs`（追加集成测试）或 `src/html_gen.rs` 内单测（若现有测试风格允许）

**Interfaces:**
- Consumes: Task 1 的 `load_manifest`；Task 2 后磁盘上的 `.glb.enc` + keys.json
- Produces: `glbData` 条目结构变为 `{"file": ..., "url": "./previews_glb/<name>.glb.enc", "key": "b64", "iv": "b64"}`（无密钥条目的旧文件保持 `{"file": ..., "url": "./previews_glb/<name>.glb"}` 明文回退）；模板查看器改为 fetch→解密→parse

- [ ] **Step 1: `generate_index_html` 的 glb_map 构建（当前 html_gen.rs:48-62）**

改为：读取 `web_root/previews_glb/keys.json`（`load_manifest`，缺失为空 map）；对每个 .blend 条目：
```rust
                let enc_name = format!("{}.glb.enc", glb_name);
                let enc_rel = format!("./previews_glb/{}", enc_name);
                let manifest = &manifest; // 顶层已 load
                if let Some(info) = manifest.get(&glb_name) {
                    glb_map.entry(entry.id.clone())
                        .or_insert_with(Vec::new)
                        .push(serde_json::json!({"file": file, "url": enc_rel, "key": info.key, "iv": info.iv}));
                } else {
                    let plain_rel = format!("./previews_glb/{}.glb", glb_name);
                    glb_map.entry(entry.id.clone())
                        .or_insert_with(Vec::new)
                        .push(serde_json::json!({"file": file, "url": plain_rel}));
                }
```
（顶层在函数开头：`let glb_dir = web_root.join("previews_glb"); let manifest = crate::glb_crypto::load_manifest(&glb_dir).unwrap_or_default();`）

- [ ] **Step 2: 模板查看器 JS（当前 html_gen.rs:546-568 `loader.load(url, ...)`）**

替换为：
```js
  function b64ToBuf(b64) {
    var bin = atob(b64);
    var buf = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
    return buf.buffer;
  }

  function loadEncrypted(url, key, iv, onLoad, onError) {
    fetch(url).then(function(res) {
      if (!res.ok) throw new Error('HTTP ' + res.status);
      return res.arrayBuffer();
    }).then(function(buf) {
      return crypto.subtle.importKey('raw', b64ToBuf(key), { name: 'AES-GCM' }, false, ['decrypt'])
        .then(function(k) {
          return crypto.subtle.decrypt({ name: 'AES-GCM', iv: b64ToBuf(iv) }, k, buf);
        });
    }).then(function(plain) {
      var GL = window.GLTFLoader || THREE.GLTFLoader;
      var loader = new GL();
      loader.parse(plain, '', function(gltf) { onLoad(gltf); }, onError);
    }).catch(function(err) {
      console.error('GLB load failed:', err);
      onError(err);
    });
  }
```
并把调用处（原 `loader.load(url, ...)`）改为：
```js
  if (url.indexOf('.glb.enc') >= 0 && key && iv) {
    loadEncrypted(url, key, iv, function(gltf) { ...原有 onLoad 逻辑... }, function(err) { ...原有 showError 逻辑... });
  } else {
    var GL = window.GLTFLoader || THREE.GLTFLoader;
    var loader = new GL();
    loader.load(url, function(gltf) { ...原有 onLoad 逻辑... }, null, function(err) { ...原有 showError 逻辑... });
  }
```
（onLoad 回调体必须与原 `loader.load` 的 onLoad 完全一致：Box3 缩放、居中、saveMaterials/applyShading/resetFrontView；onError 走 `showError('3D 模型解析失败')`。）
注意：查看器所在函数需要同时能拿到 `g.key`/`g.iv` —— 检查当前模板里 `model-box` 的数据传递方式（`data-url`），改为 `data-url` + `data-key` + `data-iv` 并在初始化函数里读取。

- [ ] **Step 3: 集成测试**

在 `tests/cli_tests.rs` 追加（遵循现有测试风格：`$env:TEMP/keinestamp_int_*` 建真实仓库成本高，此处用轻量方式：构造最小 timeline + 伪造 previews_glb/.enc + keys.json，调用 `keinestamp::html_gen::generate_index_html` 断言输出）：
```rust
#[test]
fn html_gen_embeds_enc_urls_with_keys() {
    use keinestamp::config::{MetaConfig, load_or_create_meta};
    // 用临时目录构造 MetaConfig（参考现有测试写法）
    // 1) previews_glb/abc_hello.glb.enc 存在
    // 2) keys.json 含 abc_hello -> {key, iv}
    // 3) timeline.json 含一条含 .blend 文件的 entry
    // 4) 调用 generate_index_html
    // 断言：输出 HTML 包含 './previews_glb/abc_hello.glb.enc' 与 key/iv 的 base64
}

#[test]
fn html_gen_falls_back_to_plain_url_without_manifest() {
    // 同构造，但 keys.json 缺失/无该条目
    // 断言：输出 HTML 包含 './previews_glb/abc_hello.glb' 且不含 .enc
}
```
（若 html_gen 测试需要真实 timeline 结构，参考 `src/timeline.rs` 的 Timeline/TimelineEntry 结构体构造。）

- [ ] **Step 4: 运行测试验证**

Run: `cargo test`
Expected: 新增 2 个集成测试通过；既有 67+7 无回归。

- [ ] **Step 5: Commit**

```bash
git add src/html_gen.rs tests/cli_tests.rs
git commit -m "feat(glb): 网页查看器支持密文加载（内嵌密钥 + WebCrypto 解密）"
```

---

### Task 4: Tauri 适配 + 收尾验证

**Files:**
- Modify: `src-tauri/src/lib.rs`（`get_glb_preview` 快速路径 ~line 587-592）
- Modify: `AGENTS.md`（更新架构描述）
- 验证: 全量测试 + 手动部署清单

**Interfaces:**
- Consumes: Task 1-3 全部产物
- Produces: `get_glb_preview` 快速路径查找 `.glb.enc`，返回 `.enc` 路径；接口签名不变

- [ ] **Step 1: `get_glb_preview` 快速路径（lib.rs:587-592）**

```rust
    let glb_dir = meta.web_root.join("previews_glb");
    let enc_path = glb_dir.join(format!("{}.glb.enc", glb_name));

    if enc_path.exists() {
        return Ok(enc_path.to_string_lossy().to_string());
    }
```
（重导路径经 `export_to_glb` 自动加密，无需额外改动。）

- [ ] **Step 2: 全量验证**

Run: `cargo test`
Expected: 全部通过（67+7 基线 + Task 1/3 新增）。
Run: `cd frontend; npm run check`
Expected: 0 errors（前端无改动，仅确认无破坏）。

- [ ] **Step 3: 手动部署验证清单（无法自动化的部分，留给用户执行）**

1. `keinestamp record`（或 GUI 记录）一条含 .blend 的进度
2. 确认 `web-timeline/previews_glb/` 下只有 `.glb.enc` + `keys.json`，无明文 `.glb`
3. `keinestamp deploy` 部署
4. 浏览器打开 `/<project>/`，3D 模型正常渲染（解密无感）
5. 直接访问 `/<project>/previews_glb/xxx.glb.enc` 下载 → 文件是密文（`glb_name` 与 `GLB` 魔数 `glTF` 开头对比不符）
6. 旧明文场景（可选）：手工放一个 `.glb`（无 manifest 条目）→ 重新生成网页 → 该模型仍以明文 URL 加载

- [ ] **Step 4: 更新 AGENTS.md**

在架构/相关段落补充：
- GLB 加密：`glb_crypto.rs` 模块（AES-256-GCM 每文件随机密钥）；`export_to_glb` 导出即加密（`.glb.enc` + 删明文）；`keys.json` 密钥清单随部署；html_gen 内嵌密钥、浏览器 WebCrypto 解密后 `GLTFLoader.parse`；旧明文文件回退明文 URL；`clean_old_glb_previews` 同时清理 enc 并修剪 manifest；定位"防君子"（不防逆向）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs AGENTS.md
git commit -m "feat(glb): get_glb_preview 适配密文并更新文档"
```

---

## Self-Review（已执行）

- **Spec 覆盖**：加密模块 → Task 1；导出管线接入（含跳过检查、清理、manifest 修剪）→ Task 2；页面侧（URL/密钥内嵌 + WebCrypto 解密 + 明文回退）→ Task 3；Tauri 适配 + 文档 → Task 4；serve.rs/push.rs 零改动、无水印、无服务端解密 → Global Constraints。
- **占位符扫描**：无 TBD/TODO；Task 3 Step 2 的 onLoad/onError 回调体明确要求与原逻辑一致（模板内 900 行 JS 无法整段粘贴，已指定精确行为与位置）。
- **类型一致性**：`encrypt_glb`/`update_manifest`/`load_manifest`/`prune_manifest` 签名在 Task 1 定义、Task 2 引用一致；`GlbKeyInfo { key, iv }` 贯穿三任务；Task 2 明确修正了"显式两步加密以取回密钥"的接口冲突（`encrypt_glb` 不返回密钥 → 实际加密用显式两步，`encrypt_glb` 可保留供测试用或删除——以 Task 2 实现为准，若删除则 Task 1 中同步删除其单测）。
- **注意点（Task 1→2 接口矛盾）**：`encrypt_glb` 生成密钥后不返回，Task 2 的 manifest 写入需要同一密钥。已规定 Task 2 采用显式两步（generate_key_iv → encrypt → 写文件 → 删明文 → update_manifest）。`encrypt_glb` 保留用于其单测（验证删明文+写 .enc），后续若不再需要可在 Task 2 一并删除并移除其单测。
