#[cfg(windows)]
pub fn hide_console(mut cmd: std::process::Command) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[cfg(not(windows))]
pub fn hide_console(cmd: std::process::Command) -> std::process::Command {
    cmd
}

/// 运行命令并排空 stdout/stderr 管道，支持超时 kill。
/// 不 drain 管道会在子进程输出超过管道缓冲（~64KB）时写阻塞假死；两线程持续读取直到 EOF。
/// 返回 (退出状态, stdout, stderr)。spawn 失败时 io::Error 保留在错误链中（可用 downcast_ref 取回）。
pub fn run_with_timeout(
    cmd: &mut std::process::Command,
    timeout: std::time::Duration,
    label: &str,
) -> anyhow::Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    use std::io::Read;
    let mut child = cmd.spawn()?;
    let stdout_handle = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        })
    });
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    // 先回收 reader 线程（kill 后管道 EOF，join 立即返回）再报超时
                    join_drain(stdout_handle, stderr_handle);
                    anyhow::bail!("{} 超时（{}s），已终止", label, timeout.as_secs());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                join_drain(stdout_handle, stderr_handle);
                anyhow::bail!("{} wait failed: {}", label, e);
            }
        }
    };
    let stdout = stdout_handle
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_handle
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    Ok((status, stdout, stderr))
}

fn join_drain(
    stdout: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
) {
    if let Some(h) = stdout {
        let _ = h.join();
    }
    if let Some(h) = stderr {
        let _ = h.join();
    }
}

/// 文件 SHA-256（流式读取，大文件不整块进内存）。用于第三方下载产物的完整性记录/校验。
pub fn file_sha256_hex(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 原子写：同目录唯一临时文件（pid + nanos 后缀，防并发写同一目标时临时名碰撞）+ fsync + rename。
pub fn atomic_write(path: &std::path::Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };
    std::fs::create_dir_all(&parent)?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = parent.join(format!(
        ".keinestamp-{}-{}-{}",
        std::process::id(),
        nanos,
        file_name
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content)?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn test_file_sha256_hex_known_vector() {
        let dir = std::env::temp_dir().join(format!("ks_sha_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.txt");
        std::fs::write(&f, b"abc").unwrap();
        let got = super::file_sha256_hex(&f).unwrap();
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "SHA-256 必须与标准实现一致（否则 pin 机制形同虚设）"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
