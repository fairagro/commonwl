pub trait Identifiable {
    fn get_id(&self) -> Option<&String>;
    fn set_id(&mut self, value: &str);
}

#[cfg(feature = "derive")]
pub use salad_derive::Identifiable;