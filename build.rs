//! 构建脚本：编译期捆绑 Lore v0.8.6（lore + loreserver，按编译平台选 asset）。
//! 捆绑缺失/下载失败/校验失败 → panic（构建失败，无宽松模式——Lore 是前置依赖）。
//! 产物：`src-tauri/resources/lore-bin/`（lore(.exe) + loreserver(.exe) + version.txt + LICENSE）
//! 输出编译期环境变量 `KEINESTAMP_BUNDLED_LORE_DIR` 供运行时查找。

use std::io::Read;
use std::path::{Path, PathBuf};

const LORE_VERSION: &str = "0.8.6";
const RELEASE_BASE: &str = "https://github.com/EpicGames/lore/releases/download/v0.8.6";

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
    let lore_path = out_dir.join(lore_name);
    let version_file = out_dir.join("version.txt");

    // 已捆绑且版本匹配 → 跳过（rerun-if-changed 控制何时重跑）
    if lore_path.exists()
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
        let url = format!("{}/{}", RELEASE_BASE, asset.name);
        let dest = tmp.join(asset.name);
        download(&url, &dest);
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

fn download(url: &str, dest: &Path) {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(300))
        .call()
        .unwrap_or_else(|e| panic!("下载失败 {}: {}", url, e));
    let mut reader = resp.into_reader();
    let mut body = Vec::new();
    reader
        .read_to_end(&mut body)
        .unwrap_or_else(|e| panic!("读取下载内容失败 {}: {}", url, e));
    std::fs::write(dest, &body).unwrap_or_else(|e| panic!("写入下载文件失败 {}: {}", dest.display(), e));
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
