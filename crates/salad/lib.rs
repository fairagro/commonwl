#[cfg(feature = "derive")]
pub use commonwl_salad_derive::Identifiable;

pub trait Identifiable {
    fn get_id(&self) -> Option<&String>;
    fn set_id(&mut self, value: &str);
}

pub mod deserialize;
pub mod dsl;
