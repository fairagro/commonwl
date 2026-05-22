#[test]
fn test_derive_identifiable() {
    let t = trybuild::TestCases::new();
    t.pass("tests/fixtures/valid_struct.rs");
    t.compile_fail("tests/fixtures/not_a_struct.rs");
    t.compile_fail("tests/fixtures/missing_id_field.rs");
}
