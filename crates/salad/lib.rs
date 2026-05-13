#[cfg(feature = "derive")]
pub use cwl_salad_derive::Identifiable;
use serde_json::{Map, Value};

pub trait Identifiable {
    fn get_id(&self) -> Option<&String>;
    fn set_id(&mut self, value: &str);
}

pub mod deserialize;
pub mod dsl;

pub type Mapping = Map<String, Value>;
