use std::path::PathBuf;
use std::process::Command;

fn keinestamp_bin() -> PathBuf {
    let var_name = "CARGO_BIN_EXE_keinestamp";
    if let Ok(path) = std::env::var(var_name) {
        return PathBuf::from(path);
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    if cfg!(windows) {
        path.push("keinestamp.exe");
    } else {
        path.push("keinestamp");
    }
    path
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("keinestamp_int_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> (String, bool) {
    let output = Command::new(keinestamp_bin())
        .args(args)
        .output()
        .expect("failed to run keinestamp");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let success = output.status.success();
    (stdout, success)
}

#[test]
fn test_init_creates_structure() {
    let dir = tmp_dir("init_structure");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);
    assert!(dir.join(".keinestamp.toml").exists());
    assert!(dir.join(".git").exists());
    assert!(dir.join("web-timeline").exists());
    assert!(dir.join("web-timeline/.git").exists());
    assert!(dir.join("web-timeline/timeline.json").exists());
    assert!(dir.join("web-timeline/index.html").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_commit_and_log() {
    let dir = tmp_dir("commit_log");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"hello").unwrap();

    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "first commit"]);
    assert!(ok, "record failed: {}", out);
    assert!(out.contains("已记录"));

    let (out, ok) = run(&["log", &dir.to_string_lossy(), "--count", "5"]);
    assert!(ok, "log failed: {}", out);
    assert!(out.contains("first commit"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_status() {
    let dir = tmp_dir("status");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    let (out, ok) = run(&["status", &dir.to_string_lossy()]);
    assert!(ok, "status failed: {}", out);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_config_show() {
    let dir = tmp_dir("config_show");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    let (out, ok) = run(&["config", &dir.to_string_lossy()]);
    assert!(ok, "config failed: {}", out);
    assert!(out.contains("当前配置"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_config_set() {
    let dir = tmp_dir("config_set");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    let (out, ok) = run(&[
        "config",
        &dir.to_string_lossy(),
        "--set",
        "web_remote=https://gitea.example.com/user/project-web.git",
    ]);
    assert!(ok, "config set failed: {}", out);
    assert!(out.contains("已更新"));

    let (out, ok) = run(&["config", &dir.to_string_lossy()]);
    assert!(ok, "config show failed: {}", out);
    assert!(out.contains("https://gitea.example.com/user/project-web.git"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_revert() {
    let dir = tmp_dir("revert");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["commit", &dir.to_string_lossy(), "-m", "first"]);
    assert!(ok, "commit failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["commit", &dir.to_string_lossy(), "-m", "second"]);
    assert!(ok, "second commit failed: {}", out);

    let (out, ok) = run(&["revert", &dir.to_string_lossy()]);
    assert!(ok, "revert failed: {}", out);
    assert!(out.contains("已撤回"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_export_with_real_hash() {
    let dir = tmp_dir("export_real");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("model.blend"), b"blend data").unwrap();
    let (out, ok) = run(&["commit", &dir.to_string_lossy(), "-m", "first"]);
    assert!(ok, "commit failed: {}", out);

    let timeline_path = dir.join("web-timeline").join("timeline.json");
    let content = std::fs::read_to_string(&timeline_path).unwrap();
    let timeline: serde_json::Value = serde_json::from_str(&content).unwrap();
    let first_id = timeline["entries"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let export_dir = dir.join("snapshot");
    let (out, ok) = run(&[
        "export",
        &dir.to_string_lossy(),
        "--entry",
        &first_id,
        "--output",
        &export_dir.to_string_lossy(),
    ]);
    assert!(ok, "export failed: {}", out);
    assert!(export_dir.join("files").exists());
    assert!(export_dir.join("snapshot.json").exists());
    assert!(export_dir.join("README.txt").exists());
    assert!(export_dir.join("files/model.blend").exists());

    std::fs::remove_dir_all(&dir).unwrap();
}
