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

fn require_server() {
    if !keinestamp::lore_backend::server_available() {
        panic!("loreserver 未运行：请先启动 loreserver.exe（demo 模式）");
    }
}

fn tmp_dir(name: &str) -> PathBuf {
    require_server();
    let dir = std::env::temp_dir().join(format!("keinestamp_int_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> (String, bool) {
    let output = Command::new(keinestamp_bin())
        .env("KEINESTAMP_NO_RECENT", "1")
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
    assert!(dir.join(".lore").exists(), "init 应创建 lore 仓库");
    assert!(!dir.join(".git").exists(), "不应创建 git 仓库");
    assert!(!dir.join(".gitattributes").exists(), "不应生成 .gitattributes（无 LFS）");
    assert!(dir.join("web").exists());
    assert!(!dir.join("web/.git").exists(), "web 产物并入主仓库，不应有独立 git 仓库");
    assert!(dir.join("web/timeline.json").exists());
    assert!(dir.join("web/index.html").exists());
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
    assert!(out.contains("已提交"), "record 应输出已提交: {}", out);

    std::fs::write(dir.join("test.blend"), b"hello2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "second commit"]);
    assert!(ok, "second record failed: {}", out);

    let (out, ok) = run(&["log", &dir.to_string_lossy(), "--count", "5"]);
    assert!(ok, "log failed: {}", out);
    assert!(out.contains("first commit"), "log 应包含 first commit: {}", out);
    assert!(out.contains("second commit"), "log 应包含 second commit: {}", out);
    assert!(out.contains("(1 文件)"), "log 应显示文件数: {}", out);
    assert_eq!(out.matches("first commit").count() + out.matches("second commit").count(), 2);

    let timeline_path = dir.join("web").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&timeline_path).unwrap();
    assert_eq!(timeline.entries.len(), 2, "timeline 缓存应有 2 条记录");
    // v1 语义：entries 旧→新（最新在尾部）
    assert_eq!(timeline.entries[0].message, "first commit");
    assert_eq!(timeline.entries[1].message, "second commit");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_init_then_status() {
    let dir = tmp_dir("status");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    // init 后工作区应干净（web/ 与 .keinestamp.toml 均隐藏）
    let (out, ok) = run(&["status", &dir.to_string_lossy()]);
    assert!(ok, "status failed: {}", out);
    assert!(out.contains("工作区干净"), "init 后应干净: {}", out);

    std::fs::write(dir.join("a.txt"), b"x").unwrap();
    let (out, ok) = run(&["status", &dir.to_string_lossy()]);
    assert!(ok, "status failed: {}", out);
    assert!(out.contains("a.txt"), "status 应列出新文件: {}", out);
    assert!(!out.contains("web/"), "status 不应列出隐藏的 web/: {}", out);
    assert!(!out.contains(".keinestamp.toml"), "status 不应列出隐藏的 .keinestamp.toml: {}", out);

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

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "third"]);
    assert!(ok, "third record failed: {}", out);

    let timeline = keinestamp::timeline::read_timeline(&dir.join("web").join("timeline.json")).unwrap();
    let second_id = timeline.entries.get(1).unwrap().id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &second_id[..7]]);
    assert!(ok, "rollback failed: {}", out);
    assert!(out.contains("已回退"), "应提示已回退: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v2", "回退到 second 应为 second 状态");

    let (out, ok) = run(&["restore", &dir.to_string_lossy()]);
    assert!(ok, "restore failed: {}", out);
    assert!(out.contains("已恢复"), "应提示已恢复: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v3", "恢复应回到回退前状态");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_then_record_supersedes_after() {
    let dir = tmp_dir("cli_rollback_record");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "C"]);
    assert!(ok, "record C failed: {}", out);

    let tl_path = dir.join("web").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    let b_id = timeline.entries.get(1).unwrap().id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback failed: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v2", "回退到 B 应为 B 状态");

    std::fs::write(dir.join("test.blend"), b"v4").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "D"]);
    assert!(ok, "record D failed: {}", out);

    // 回退到 B 后记录 D：B 及更早保留，C 被顶替出主线（entries 旧→新，最新在尾部）
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    let messages: Vec<&str> = timeline.entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, vec!["A", "B", "D"], "C 应被顶替: {:?}", messages);

    let (out, ok) = run(&["log", &dir.to_string_lossy(), "--count", "10"]);
    assert!(ok, "log failed: {}", out);
    assert!(out.contains("A"), "log 应包含 A: {}", out);
    assert!(out.contains("B"), "log 应包含 B: {}", out);
    assert!(out.contains("D"), "log 应包含 D: {}", out);
    assert!(!out.contains("C"), "log 不应再有 C: {}", out);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_first_entry() {
    let dir = tmp_dir("cli_rollback_first");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    let tl_path = dir.join("web").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    // entries 旧→新（最新在尾部）
    let a_id = timeline.entries.get(0).unwrap().id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &a_id[..7]]);
    assert!(ok, "rollback first failed: {}", out);
    assert!(out.contains("已回退"), "应提示已回退: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v1", "回退首条记录 = 首条状态");

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "C"]);
    assert!(ok, "record C failed: {}", out);

    // 首条记录后记录 C：A 保留，B 被顶替出主线（entries 旧→新）
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    let messages: Vec<&str> = timeline.entries.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, vec!["A", "C"], "B 应被顶替: {:?}", messages);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cli_rollback_while_in_rollback_auto_restores() {
    let dir = tmp_dir("cli_rollback_chain");
    let (out, ok) = run(&["init", &dir.to_string_lossy()]);
    assert!(ok, "init failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v1").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "A"]);
    assert!(ok, "record A failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v2").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "B"]);
    assert!(ok, "record B failed: {}", out);

    std::fs::write(dir.join("test.blend"), b"v3").unwrap();
    let (out, ok) = run(&["record", &dir.to_string_lossy(), "-m", "C"]);
    assert!(ok, "record C failed: {}", out);

    let tl_path = dir.join("web").join("timeline.json");
    let timeline = keinestamp::timeline::read_timeline(&tl_path).unwrap();
    // entries 旧→新（最新在尾部）
    let a_id = timeline.entries.get(0).unwrap().id.clone();
    let b_id = timeline.entries.get(1).unwrap().id.clone();

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &b_id[..7]]);
    assert!(ok, "rollback B failed: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v2", "回退 B 后应为 B 状态");

    let (out, ok) = run(&["rollback", &dir.to_string_lossy(), "--entry", &a_id[..7]]);
    assert!(ok, "回退状态下再回退 A 应自动恢复后回退: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v1", "回退 A 后应为 A 状态");

    let st = keinestamp::lore_project::rollback_state(&dir).unwrap();
    assert!(st.active, "应处于回退状态");
    assert_eq!(st.revision.as_deref(), Some(timeline.entries[0].id.as_str()), "回退点应为 A");

    let (out, ok) = run(&["restore", &dir.to_string_lossy()]);
    assert!(ok, "restore failed: {}", out);
    assert_eq!(std::fs::read(dir.join("test.blend")).unwrap(), b"v3", "恢复应回到 C 状态");

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

    let timeline_path = dir.join("web").join("timeline.json");
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
    assert_eq!(std::fs::read(export_dir.join("files/model.blend")).unwrap(), b"blend data");

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
            tag: Some("建模".to_string()),
            milestone: false,
        }],
        rollback_marker: None,
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
