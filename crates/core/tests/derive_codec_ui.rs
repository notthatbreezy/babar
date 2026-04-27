//! UI coverage for `#[derive(Codec)]` diagnostics.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn derive_codec_ui() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/derive/pass/*.rs");
    tests.compile_fail("tests/ui/derive/fail/ambiguous.rs");
    tests.compile_fail("tests/ui/derive/fail/generic.rs");
    tests.compile_fail("tests/ui/derive/fail/missing_attr.rs");
    tests.compile_fail("tests/ui/derive/fail/tuple_struct.rs");
}

#[test]
fn derive_codec_wrong_type_reports_mismatch() {
    let temp_dir = create_temp_project_dir();
    fs::create_dir_all(temp_dir.join("src")).expect("create temp src dir");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui/derive/fail/wrong_type.rs");
    let source = fs::read_to_string(&fixture).expect("read wrong_type fixture");
    fs::write(temp_dir.join("src/main.rs"), source).expect("write temp main.rs");

    let manifest = format!(
        r#"[package]
name = "derive-codec-wrong-type"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
babar = {{ path = "{}" }}
"#,
        env!("CARGO_MANIFEST_DIR"),
    );
    fs::write(temp_dir.join("Cargo.toml"), manifest).expect("write temp Cargo.toml");

    let target_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/custom-ui");
    let output = Command::new("cargo")
        .arg("check")
        .current_dir(&temp_dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .expect("run cargo check for wrong_type fixture");

    fs::remove_dir_all(&temp_dir).ok();

    assert!(
        !output.status.success(),
        "wrong_type fixture should fail to compile\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    for needle in [
        "Int4Codec: Encoder<String>",
        "Int4Codec: Decoder<String>",
        "expected `i32`, found `String`",
    ] {
        assert!(
            stderr.contains(needle),
            "wrong_type fixture stderr should contain `{needle}`\nfull stderr:\n{stderr}",
        );
    }
}

fn create_temp_project_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/custom-ui-projects")
        .join(format!("derive-codec-wrong-type-{unique}"))
}
