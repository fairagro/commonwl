use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_yaml::Value;
use std::collections::HashMap;

pub trait FromShortHand {
    fn from_shorthand(_key: &str, _value: Value) -> Option<Value> {
        None
    }
}

fn deserialize_map_list<'de, D, T>(deserializer: D, tag: &str) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + FromShortHand + Clone,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TFormat<T> {
        Array(Vec<T>),
        Map(HashMap<String, Value>),
    }

    match TFormat::deserialize(deserializer)? {
        TFormat::Array(items) => Ok(items),
        TFormat::Map(mut map) => {
            let mut result = vec![];
            for (key, value) in map.iter_mut() {
                let normalized = match value {
                    Value::Mapping(m) => {
                        m.insert(
                            Value::String(tag.to_string()),
                            Value::String(key.to_string()),
                        );
                        Value::Mapping(m.clone())
                    }
                    _ => {
                        if let Some(sh) = T::from_shorthand(key, value.to_owned()) {
                            sh
                        } else {
                            Err(serde::de::Error::custom("From Shorthand returned None"))?
                        }
                    }
                };
                result.push(normalized);
            }
            Ok(serde_yaml::from_value(serde_yaml::Value::Sequence(result))
                .map_err(serde::de::Error::custom)?)
        }
    }
}

macro_rules! make_deserialize_map_list {
    ($func_name:ident, $tag:expr) => {
        pub fn $func_name<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: DeserializeOwned + FromShortHand + Clone,
        {
            deserialize_map_list::<D, T>(deserializer, $tag)
        }
    };
}

macro_rules! make_deserialize_map_list_option {
    ($func_name:ident, $tag:expr) => {
        pub fn $func_name<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
        where
            D: Deserializer<'de>,
            T: DeserializeOwned + FromShortHand + Clone,
        {
            let result = deserialize_map_list::<D, T>(deserializer, $tag)?;
            if result.is_empty() {
                Ok(None)
            } else {
                Ok(Some(result))
            }
        }
    };
}

make_deserialize_map_list!(deserialize_map_list_class, "class");
make_deserialize_map_list!(deserialize_map_list_id, "id");
make_deserialize_map_list!(deserialize_map_list_package, "package");
make_deserialize_map_list!(deserialize_map_list_envname, "envName");
make_deserialize_map_list_option!(deserialize_map_list_option_name, "name");

macro_rules! make_shorthand_impl {
    ($class:ident, $id:expr, $type:expr) => {
        impl FromShortHand for $class {
            fn from_shorthand(key: &str, value: serde_yaml::Value) -> Option<serde_yaml::Value> {
                let mut map = serde_yaml::Mapping::new();
                map.insert($id.into(), serde_yaml::Value::String(key.to_owned()));
                map.insert($type.into(), value);
                Some(serde_yaml::Value::Mapping(map))
            }
        }
    };
}
pub(crate) use make_shorthand_impl;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unused)]
    fn test_shorthand_serialization() {
        #[derive(Deserialize, Debug, Clone)]
        enum TheType {
            Art,
            Music,
        }

        #[derive(Deserialize, Debug, Clone)]

        struct DaInput {
            id: String,
            #[serde(rename = "type")]
            type_: TheType,
        }
        make_shorthand_impl!(DaInput, "id", "type");

        #[derive(Deserialize, Debug)]
        struct DaInputBag {
            #[serde(deserialize_with = "deserialize_map_list_id")]
            inputs: Vec<DaInput>,
        }

        let shorthand = r#"
        inputs:
          my_id: Music
        "#;
        let res = serde_yaml::from_str::<DaInputBag>(shorthand);
        assert!(res.is_ok());
    }
}
