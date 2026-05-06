//! UI coverage for public `query!` / `command!` migration and verification behavior.

mod common;

use std::panic::{self, AssertUnwindSafe};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use babar::Session;
use common::{AuthMode, PgContainer};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_env(vars: &[(&str, Option<String>)], f: impl FnOnce()) {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let saved: Vec<_> = vars
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
        .collect();

    for (key, value) in vars {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    let result = panic::catch_unwind(AssertUnwindSafe(f));

    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(&key, value),
            None => std::env::remove_var(&key),
        }
    }

    if let Err(payload) = result {
        panic::resume_unwind(payload);
    }
}

fn require_docker() -> bool {
    let ok = Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !ok {
        eprintln!("skipping: docker unavailable");
    }
    ok
}

fn typed_query_alias_removed_fixture() -> &'static str {
    let rustc = Command::new("rustc")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok());

    let is_rust_1_88 = rustc
        .as_deref()
        .and_then(|version| version.split_whitespace().nth(1))
        .is_some_and(|version| version.starts_with("1.88."));

    if is_rust_1_88 {
        "tests/ui/typed_macro/fail/typed_query_alias_removed_rust_1_88.rs"
    } else {
        "tests/ui/typed_macro/fail/typed_query_alias_removed.rs"
    }
}

#[test]
fn typed_macro_ui() {
    with_env(
        &[("BABAR_DATABASE_URL", None), ("DATABASE_URL", None)],
        || {
            let tests = trybuild::TestCases::new();
            tests.pass("tests/ui/typed_macro/pass/basic.rs");
            tests.pass("tests/ui/typed_macro/pass/struct_shape_selection.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/legacy_command_syntax.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/legacy_query_syntax.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_extra_params.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_extra_row.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_missing_params.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_missing_row.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_selection_precedence.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_type_mismatch_params.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_type_mismatch_row.rs");
            tests.compile_fail("tests/ui/typed_macro/fail/struct_shape_ambiguous_row_names.rs");
            tests.compile_fail(typed_query_alias_removed_fixture());
        },
    );
}

#[test]
fn typed_macro_reports_configuration_errors_for_verifiable_codecs() {
    with_env(
        &[(
            "BABAR_DATABASE_URL",
            Some("definitely not a postgres url".into()),
        )],
        || {
            let tests = trybuild::TestCases::new();
            tests.compile_fail("tests/ui/typed_macro/fail/verify_invalid_config.rs");
        },
    );
}

#[test]
fn typed_macro_verifies_against_live_postgres_when_configured() {
    if !require_docker() {
        return;
    }

    let runtime = tokio::runtime::Runtime::new().expect("create runtime");
    let pg = runtime.block_on(PgContainer::start(AuthMode::Scram));
    let session = runtime
        .block_on(Session::connect(pg.config(pg.user(), pg.password())))
        .expect("connect for schema setup");
    runtime
        .block_on(session.simple_query_raw(
            "CREATE TABLE public.verify_live_users (\
                 id int4 PRIMARY KEY, \
                 name text NOT NULL, \
                 active bool NOT NULL\
             )",
        ))
        .expect("create verification table");
    runtime
        .block_on(session.close())
        .expect("close setup session");
    let database_url = format!(
        "postgres://{}:{}@127.0.0.1:{}/babar",
        pg.user(),
        pg.password(),
        pg.port()
    );

    with_env(&[("BABAR_DATABASE_URL", Some(database_url))], || {
        let tests = trybuild::TestCases::new();
        tests.pass("tests/ui/typed_macro/pass/verify_live_ok.rs");
        tests.compile_fail("tests/ui/typed_macro/fail/verify_live_param_mismatch.rs");
        tests.compile_fail("tests/ui/typed_macro/fail/verify_live_row_mismatch.rs");
    });
}
