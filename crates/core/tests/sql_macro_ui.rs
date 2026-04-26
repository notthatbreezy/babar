//! UI coverage for `sql!` diagnostics.

#[test]
fn sql_macro_ui() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/sql/pass/*.rs");
    tests.compile_fail("tests/ui/sql/fail/*.rs");
}
