//! UI coverage for `#[derive(Codec)]` diagnostics.

#[test]
fn derive_codec_ui() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/derive/pass/*.rs");
    tests.compile_fail("tests/ui/derive/fail/*.rs");
}
