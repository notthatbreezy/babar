//! UI coverage for `typed_query!` diagnostics.

#[test]
fn typed_query_ui() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/typed_query/pass/basic.rs");
    tests.compile_fail("tests/ui/typed_query/fail/unsupported_type.rs");
    tests.compile_fail("tests/ui/typed_query/fail/unknown_column.rs");
}
