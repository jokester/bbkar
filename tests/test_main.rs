use std::process::Command;

fn bbkar_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bbkar")
}

fn write_config(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let source_path = dir.path().join("source");
    let dest_path = dir.path().join("dest");
    std::fs::create_dir_all(&source_path).unwrap();
    std::fs::create_dir_all(&dest_path).unwrap();

    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[global]

[source.src]
path = "{}"

[dest.dst]
driver = "local"
path = "{}"

[sync.main]
source = "src"
dest = "dst"
filter = ["*"]
"#,
            source_path.display(),
            dest_path.display()
        ),
    )
    .unwrap();

    config_path
}

#[test]
fn test_main_requires_config_flag() {
    let output = Command::new(bbkar_bin()).arg("status").output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ERROR --config <CONFIG_FILE> is required"));
}

#[test]
fn test_main_rejects_duplicate_config_flags() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = write_config(&tmp);

    let output = Command::new(bbkar_bin())
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "--config",
            config_path.to_str().unwrap(),
            "status",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("the argument '--config <CONFIG_FILE>' cannot be used multiple times"));
}

#[test]
fn test_main_dispatches_status_command() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = write_config(&tmp);

    let output = Command::new(bbkar_bin())
        .args(["--config", config_path.to_str().unwrap(), "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("bbkar status"));
    assert!(combined.contains("[sync.main]"));
}
