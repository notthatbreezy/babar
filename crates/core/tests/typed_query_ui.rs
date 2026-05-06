//! UI coverage for schema-aware typed SQL diagnostics.

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

fn rust_1_88_trybuild() -> bool {
    let rustc = Command::new("rustc")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok());

    rustc
        .as_deref()
        .and_then(|version| version.split_whitespace().nth(1))
        .is_some_and(|version| version.starts_with("1.88."))
}

fn schema_scoped_aliases_removed_fixture() -> &'static str {
    if rust_1_88_trybuild() {
        "tests/ui/typed_query/fail/schema_scoped_aliases_removed_rust_1_88.rs"
    } else {
        "tests/ui/typed_query/fail/schema_scoped_aliases_removed.rs"
    }
}

fn multi_statement_fixture() -> &'static str {
    if rust_1_88_trybuild() {
        "tests/ui/typed_query/fail/multi_statement_rust_1_88.rs"
    } else {
        "tests/ui/typed_query/fail/multi_statement.rs"
    }
}

#[test]
fn public_typed_sql_ui() {
    with_env(
        &[("BABAR_DATABASE_URL", None), ("DATABASE_URL", None)],
        || {
            let tests = trybuild::TestCases::new();
            tests.pass("tests/ui/typed_query/pass/authored_schema_qualified.rs");
            tests.pass("tests/ui/typed_query/pass/authored_schema_same_table_names.rs");
            tests.pass("tests/ui/typed_query/pass/basic.rs");
            tests.pass("tests/ui/typed_query/pass/insert_command.rs");
            tests.pass("tests/ui/typed_query/pass/schema_scoped.rs");
            tests.pass("tests/ui/typed_query/pass/schema_scoped_command.rs");
            tests.pass("tests/ui/typed_query/pass/schema_scoped_struct_alias_match.rs");
            tests.pass("tests/ui/typed_query/pass/schema_scoped_struct_shape_selection.rs");
            tests.pass("tests/ui/typed_query/pass/update_returning.rs");
            tests.compile_fail("tests/ui/typed_query/fail/ambiguous_optional_ownership.rs");
            tests.compile_fail("tests/ui/typed_query/fail/authored_unknown_column.rs");
            tests.compile_fail("tests/ui/typed_query/fail/authored_unknown_table.rs");
            #[cfg(not(feature = "time"))]
            tests.compile_fail("tests/ui/typed_query/fail/authored_unsupported_declared_type.rs");
            tests.compile_fail("tests/ui/typed_query/fail/authored_unsupported_marker.rs");
            tests.compile_fail("tests/ui/typed_query/fail/delete_using.rs");
            tests.compile_fail("tests/ui/typed_query/fail/delete_without_where.rs");
            tests.compile_fail("tests/ui/typed_query/fail/invalid_optional_limit_group.rs");
            tests.compile_fail("tests/ui/typed_query/fail/invalid_optional_projection.rs");
            tests.compile_fail("tests/ui/typed_query/fail/insert_on_conflict.rs");
            tests.compile_fail("tests/ui/typed_query/fail/insert_select.rs");
            tests.compile_fail("tests/ui/typed_query/fail/mixed_inline_external.rs");
            tests.compile_fail(schema_scoped_aliases_removed_fixture());
            tests.compile_fail(
                "tests/ui/typed_query/fail/schema_scoped_struct_shape_ambiguous_row_names.rs",
            );
            tests.compile_fail(
                "tests/ui/typed_query/fail/schema_scoped_struct_shape_selection_precedence.rs",
            );
            tests.compile_fail(multi_statement_fixture());
            tests.compile_fail("tests/ui/typed_query/fail/returning_wildcard.rs");
            tests.compile_fail("tests/ui/typed_query/fail/unsupported_type.rs");
            tests.compile_fail("tests/ui/typed_query/fail/unknown_column.rs");
            tests.compile_fail("tests/ui/typed_query/fail/update_from.rs");
            tests.compile_fail("tests/ui/typed_query/fail/update_without_where.rs");
            #[cfg(feature = "time")]
            tests.pass("tests/ui/typed_query/pass/authored_timestamptz.rs");
        },
    );
}

#[test]
fn public_typed_sql_reports_configuration_errors_for_live_verification() {
    with_env(
        &[(
            "BABAR_DATABASE_URL",
            Some("definitely not a postgres url".into()),
        )],
        || {
            let tests = trybuild::TestCases::new();
            tests.compile_fail("tests/ui/typed_query/fail/verify_invalid_config.rs");
        },
    );
}

#[test]
fn public_typed_sql_verifies_referenced_schema_facts_against_live_postgres() {
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
        tests.pass("tests/ui/typed_query/pass/verify_live_ok.rs");
        tests.compile_fail("tests/ui/typed_query/fail/verify_live_schema_mismatch.rs");
    });
}
