use cwl_salad::Identifiable;

#[derive(Identifiable)]
struct User {
    id: Option<String>,
    name: String,
}

fn main() {
    let mut u = User {
        id: None,
        name: "Alice".into(),
    };
    u.set_id("abc");
    assert_eq!(u.get_id(), Some(&"abc".to_string()));
}
