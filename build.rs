//! 构建脚本：编译期捆绑 Lore v0.8.6（lore + loreserver，按编译平台选 asset）。
//! 捆绑缺失/下载失败/校验失败 → panic（构建失败，无宽松模式——Lore 是前置依赖）。
//! 产物：`src-tauri/resources/lore-bin/`（lore(.exe) + loreserver(.exe) + version.txt + LICENSE）
//! 输出编译期环境变量 `KEINESTAMP_BUNDLED_LORE_DIR` 供运行时查找。
//!
//! 国内网络 / 离线构建支持（按优先级）：
//! 1. `KEINESTAMP_LORE_ARCHIVE_DIR`：本地目录放两份官方资产压缩包（lore-…zip / loreserver-…zip），
//!    完全离线构建（仍做 sha256 校验，防篡改）；
//! 2. `KEINESTAMP_LORE_RELEASE_BASE`：自定义下载 base URL（自建镜像/内网源，布局同 GitHub Releases）；
//! 3. 默认：官方 GitHub Releases → 公共加速代理（gh-proxy.com 等）按序回退，每源重试 3 次；
//! 4. 或者直接把整份 `lore-bin`（lore(.exe)+loreserver(.exe)+version.txt+LICENSE）从其他机器
//!    拷贝到 `src-tauri/resources/lore-bin/`——构建检测到版本匹配后完全跳过下载。

use std::io::Read;
use std::path::{Path, PathBuf};

const LORE_VERSION: &str = "0.8.6";
const RELEASE_BASE: &str = "https://github.com/EpicGames/lore/releases/download/v0.8.6";

/// GitHub Release 加速代理前缀（官方源失败时按序回退；公共代理可能变动，sha256 校验保证完整性）
const RELEASE_PROXIES: [&str; 4] = [
    "https://gh-proxy.com/",
    "https://ghfast.top/",
    "https://ghproxy.net/",
    "https://gh.llkk.cc/",
];

/// asset 名 + 官方 sha256 digest（2026-07-31 发布 v0.8.6，GitHub API 查证）
struct Asset {
    name: &'static str,
    sha256: &'static str,
}

fn platform_assets() -> Vec<Asset> {
    #[cfg(target_os = "windows")]
    {
        vec![
            Asset { name: "lore-v0.8.6-x86_64-pc-windows-msvc.zip", sha256: "092f0d06dcb84a514dec4a2a87a8a854963feb951d640d8100ce3e42acd68d67" },
            Asset { name: "loreserver-v0.8.6-x86_64-pc-windows-msvc.zip", sha256: "4251e1419875c08485e1d316a1a0c5724e29e2c1a4d952beeb6bf3344926c75e" },
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            Asset { name: "lore-v0.8.6-x86_64-unknown-linux-gnu.tar.gz", sha256: "a7aaeb32a15f6674a328e19096c48dcab5b29b283ba51d37abdadedd930812ad" },
            Asset { name: "loreserver-v0.8.6-x86_64-unknown-linux-gnu.tar.gz", sha256: "f0de84c6175a476f157754f57316be0346105be502c93fabb65bb908eab0e1e1" },
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        panic!("不支持的构建平台：仅支持 Windows/Linux x86_64（Lore 捆绑）")
    }
}

fn main() {
    let out_dir = std::env::var("KEINESTAMP_LORE_BUNDLE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src-tauri")
                .join("resources")
                .join("lore-bin")
        });
    let lore_name = if cfg!(target_os = "windows") { "lore.exe" } else { "lore" };
    let loreserver_name = if cfg!(target_os = "windows") { "loreserver.exe" } else { "loreserver" };
    let lore_path = out_dir.join(lore_name);
    let loreserver_path = out_dir.join(loreserver_name);
    let version_file = out_dir.join("version.txt");

    println!("cargo:rerun-if-env-changed=KEINESTAMP_LORE_ARCHIVE_DIR");
    println!("cargo:rerun-if-env-changed=KEINESTAMP_LORE_RELEASE_BASE");
    println!("cargo:rerun-if-env-changed=KEINESTAMP_LORE_BUNDLE_DIR");

    // 已捆绑、版本匹配且两个二进制齐全 → 跳过（rerun-if-changed 控制何时重跑）
    if lore_path.exists()
        && loreserver_path.exists()
        && version_file.exists()
        && std::fs::read_to_string(&version_file)
            .map(|v| v.trim() == LORE_VERSION)
            .unwrap_or(false)
    {
        println!("cargo:warning=Lore 捆绑已就绪（{}）", out_dir.display());
        emit_env(&out_dir);
        copy_to_profile_dir(&out_dir);
        println!("cargo:rerun-if-changed={}", out_dir.display());
        return;
    }

    // 缺失 → 下载（失败/校验失败 → panic，构建失败）
    println!("cargo:warning=下载 Lore v{} 捆绑到 {} ...", LORE_VERSION, out_dir.display());
    std::fs::create_dir_all(&out_dir).expect("创建捆绑目录失败");
    let tmp = std::env::temp_dir().join(format!("keinestamp_lore_bundle_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("创建临时目录失败");

    for asset in platform_assets() {
        let dest = tmp.join(asset.name);
        fetch_asset(&asset, &dest, &out_dir);
        verify_sha256(&dest, asset.sha256, asset.name);
        extract(&dest, &tmp, asset.name);
    }

    // 收集解压产物中的二进制（zip/tar 可能带目录层级）
    for bin in ["lore", "loreserver"] {
        let exe_name = if cfg!(target_os = "windows") {
            format!("{}.exe", bin)
        } else {
            bin.to_string()
        };
        let src = find_file(&tmp, &exe_name).unwrap_or_else(|| {
            panic!("解压产物中未找到 {}", exe_name)
        });
        let dst = out_dir.join(&exe_name);
        std::fs::copy(&src, &dst).expect("复制捆绑二进制失败");
    }

    std::fs::write(&version_file, LORE_VERSION).expect("写 version.txt 失败");
    std::fs::write(out_dir.join("LICENSE"), MIT_LICENSE).expect("写 LICENSE 失败");
    let _ = std::fs::remove_dir_all(&tmp);
    println!("cargo:warning=Lore 捆绑完成（{}）", out_dir.display());
    emit_env(&out_dir);
    copy_to_profile_dir(&out_dir);
    println!("cargo:rerun-if-changed={}", out_dir.display());
}

/// 拷贝捆绑目录到构建产物目录（`target/<profile>/lore-bin`，与 exe 同目录）——
/// 分发时 exe + lore-bin 两件套直接可用（CLI 与 GUI 构建均自动执行）。
fn copy_to_profile_dir(out_dir: &Path) {
    // OUT_DIR = target/<profile>/build/keinestamp-<hash>/out → 上 3 级 = target/<profile>
    let Some(out_path) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        println!("cargo:warning=无法读取 OUT_DIR，跳过拷贝 lore-bin 到 exe 同目录");
        return;
    };
    let Some(profile) = out_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    else {
        println!("cargo:warning=无法推导 profile 目录（{}），跳过拷贝", out_path.display());
        return;
    };
    let dest = profile.join("lore-bin");
    let _ = std::fs::remove_dir_all(&dest);
    if let Err(e) = copy_dir(out_dir, &dest) {
        println!("cargo:warning=拷贝 lore-bin 到 {} 失败: {}", dest.display(), e);
        return;
    }
    println!("cargo:warning=Lore 捆绑已拷贝到 exe 同目录: {}", dest.display());
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn emit_env(out_dir: &Path) {
    println!("cargo:rustc-env=KEINESTAMP_BUNDLED_LORE_DIR={}", out_dir.display());
}

/// 获取资产压缩包：本地归档目录（离线）→ 自定义 base → 官方 GitHub → 加速代理（回退+重试）。
/// 所有来源最终都过 sha256 校验（verify_sha256），代理/镜像无法篡改内容。
fn fetch_asset(asset: &Asset, dest: &Path, out_dir: &Path) {
    // 1) 本地归档目录：完全离线构建（仍校验 sha256）
    if let Ok(dir) = std::env::var("KEINESTAMP_LORE_ARCHIVE_DIR") {
        let src = PathBuf::from(&dir).join(asset.name);
        if !src.is_file() {
            panic!(
                "KEINESTAMP_LORE_ARCHIVE_DIR 中未找到 {}（期望: {}）。请把两份官方资产压缩包放到该目录。",
                asset.name,
                src.display()
            );
        }
        std::fs::copy(&src, dest)
            .unwrap_or_else(|e| panic!("复制本地归档失败 {}: {}", src.display(), e));
        println!("cargo:warning=使用本地归档: {}", src.display());
        return;
    }
    // 2) 下载源列表：自定义 base（仅用它）→ 官方 GitHub → 加速代理前缀
    let mut bases: Vec<String> = Vec::new();
    match std::env::var("KEINESTAMP_LORE_RELEASE_BASE") {
        Ok(base) => bases.push(base),
        Err(_) => {
            bases.push(RELEASE_BASE.to_string());
            for proxy in RELEASE_PROXIES {
                bases.push(format!("{}{}/{}", proxy, RELEASE_BASE, asset.name));
            }
        }
    }
    let mut last_err = String::new();
    for (i, base) in bases.iter().enumerate() {
        let url = format!("{}/{}", base.trim_end_matches('/'), asset.name);
        println!("cargo:warning=下载 Lore 资产 [源 {}] {}", i + 1, url);
        match download_retry(&url, dest) {
            Ok(()) => return,
            Err(e) => last_err = format!("{} → {}", url, e),
        }
    }
    panic!(
        "下载 Lore 资产失败：{}。
         可尝试以下方式之一：
         1) 设置 KEINESTAMP_LORE_ARCHIVE_DIR 指向包含 {} 的本地目录（离线构建，需 sha256 一致）；
         2) 设置 KEINESTAMP_LORE_RELEASE_BASE 指向可用镜像（布局同 GitHub Releases）；
         3) 从其他机器拷贝整份 lore-bin（lore+loreserver+version.txt+LICENSE）到 {}。
         最近错误：{}",
        asset.name,
        asset.name,
        out_dir.display(),
        last_err
    );
}

/// 单 URL 下载：重试 3 次（指数退避），返回 Ok 或错误信息
fn download_retry(url: &str, dest: &Path) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 1..=3 {
        match download_once(url, dest) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                println!("cargo:warning=下载失败（第 {}/3 次）{}: {}", attempt, url, last_err);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_secs(2 * attempt));
                }
            }
        }
    }
    Err(last_err)
}

fn download_once(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(300))
        .call()
        .map_err(|e| format!("连接失败: {}", e))?;
    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut reader = resp.into_reader();
    let mut body = Vec::new();
    reader
        .read_to_end(&mut body)
        .map_err(|e| format!("读取下载内容失败: {}", e))?;
    std::fs::write(dest, &body).map_err(|e| format!("写入下载文件失败: {}", e))?;
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str, name: &str) {
    use sha2::Digest;
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("读取 {} 失败: {}", path.display(), e));
    let digest = hex::encode(sha2::Sha256::digest(&data));
    assert_eq!(digest, expected, "sha256 校验失败（可能被篡改或下载不完整）: {}", name);
}

fn extract(path: &Path, dest: &Path, name: &str) {
    if name.ends_with(".zip") {
        extract_zip(path, dest);
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_targz(path, dest);
    } else {
        panic!("未知压缩格式: {}", name);
    }
}

fn extract_zip(path: &Path, dest: &Path) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("打开 {} 失败: {}", path.display(), e));
    let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("解析 zip {} 失败: {}", path.display(), e));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap_or_else(|e| panic!("读取 zip 条目失败: {}", e));
        let rel = entry.name().trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).expect("创建解压目录失败");
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).expect("创建解压目录失败");
            }
            let mut f = std::fs::File::create(&out).expect("创建解压文件失败");
            std::io::copy(&mut entry, &mut f).expect("解压写入失败");
        }
    }
}

fn extract_targz(path: &Path, dest: &Path) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("打开 {} 失败: {}", path.display(), e));
    let gz = flate2::read::GzDecoder::new(file);
    let mut ar = tar::Archive::new(gz);
    ar.unpack(dest).unwrap_or_else(|e| panic!("解压 tar.gz 失败: {}", e));
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
    }
    None
}

/// MIT 许可声明（Lore © Epic Games, Inc. —— 捆绑分发必须附带）
const MIT_LICENSE: &str = r#"MIT License

Copyright (c) 2026 Epic Games, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the
following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO
EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
USE OR OTHER DEALINGS IN THE SOFTWARE.
"#;
