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
        "access_token=abcdef1234567890",
    ]);
    assert!(ok, "config set failed: {}", out);
    assert!(out.contains("已更新"));

    let (out, ok) = run(&["config", &dir.to_string_lossy()]);
    assert!(ok, "config show failed: {}", out);
    assert!(out.contains("abcd****"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_and_restore() {
    let dir = tmp_dir("cli_rollback");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "first"]);
    assert!(ok, "record failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "second"]);
    assert!(ok, "second record failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&dir.join("web-timeline").join("timeline.json")).unwrap();
    let second_id = timeline.entries.get(1).unwrap().id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &second_id[..7]]);
    assert!(ok, "rollback failed: {}", out);
    assert!(out.contains("已回退"));
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v1");

    let (out, ok) = run(&["restore", &dir.to_string_lossy()]);
    assert!(ok, "restore failed: {}", out);
    assert!(out.contains("已恢复"));
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v2");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_then_record_replaces() {
    let dir = tmp_dir("cli_rollback_record");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    let tl_path = dir.join("web-timeline").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    let a_id = timeline.entries[0].id.clone();
    let b_id = timeline.entries[1].id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback failed: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v1");

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "C"]);
    assert!(ok, "record C failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    assert_eq!(timeline.entries.len(), 2, "B 应被顶替: {:?}", timeline.entries.iter().map(|e| &e.message).collect::<Vec<_>>());
    assert_eq!(timeline.entries[0].id, a_id, "A 应保留");
    assert_eq!(timeline.entries[1].message, "C", "新记录 C 应接替 B");

    let log_out = keinestamp::git_project::log(&dir, 10).unwrap();
    assert!(!log_out.contains("B"), "git 历史不应再有 B: {}", log_out);
    assert!(log_out.contains("C"), "git 历史应包含 C: {}", log_out);
    assert_eq!(log_out.lines().count(), 2, "git 历史应为 A → C: {}", log_out);

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

#[test]
fn html_gen_contains_daynight_theme() {
    let dir = tmp_dir("theme");
    let timeline = keinestamp::timeline::Timeline {
        project: "ThemeTest".to_string(),
        entries: vec![keinestamp::timeline::TimelineEntry {
            id: "abc123def456".to_string(),
            timestamp: "2026-06-13T12:00:00Z".to_string(),
            author: "artist".to_string(),
            message: "theme test".to_string(),
            files: vec!["char.blend".to_string()],
            file_status: vec![],
            preview: "previews/abc123.png".to_string(),
            tag: Some("建模".to_string()), milestone: false,
        }],
    };

    keinestamp::html_gen::generate_index_html(&dir, &dir, &timeline, "ThemeTest").unwrap();

    let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
    assert!(html.contains(r#"[data-theme="night"]"#), "missing night palette selector");
    assert!(html.contains("keinestamp_web_theme"), "missing theme storage key");
    assert!(html.contains("theme-toggle"), "missing toggle button id");
    assert!(html.contains("color-mix"), "missing color-mix badge styles");
    assert!(html.contains("--bg: #151719"), "night palette drifted");
    assert!(html.contains("localStorage.getItem('keinestamp_web_theme')"), "missing pre-read script");
    assert!(!html.contains("class=\"brand\""), "web logo should be removed");
    assert!(html.contains("shadow-hover"), "missing hover shadow token");
    assert!(html.contains("--tint-soft"), "missing tint tokens");

    std::fs::remove_dir_all(&dir).unwrap();
}
