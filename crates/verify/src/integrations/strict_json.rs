//! Reject duplicate object keys at every depth. A second `failed: 0` must not
//! erase `failed: 1` before a decoder sees the report.
use super::DecodeError;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(Unique(x)) = a.next_element()? {
                    v.push(x);
                }
                Ok(Unique(Value::Array(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if v.contains_key(&k) {
                        return Err(de::Error::custom("duplicate object key"));
                    }
                    let Unique(x) = a.next_value()?;
                    v.insert(k, x);
                }
                Ok(Unique(Value::Object(v)))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn parse_json(bytes: &[u8]) -> Result<Value, DecodeError> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(DecodeError::TooLarge);
    }
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let Unique(v) = Unique::deserialize(&mut d).map_err(|e| DecodeError::Invalid(e.to_string()))?;
    d.end().map_err(|e| DecodeError::Invalid(e.to_string()))?;
    Ok(v)
}

/// Visit one array element at a time. Duplicate-key and recursion checks still
/// apply inside every element, but completed JSON subtrees are not retained.
pub(super) fn visit_array(
    bytes: &[u8],
    visitor: impl FnMut(usize, Value) -> Result<(), DecodeError>,
) -> Result<(), DecodeError> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(DecodeError::TooLarge);
    }
    struct Items<F>(F);
    impl<'de, F: FnMut(usize, Value) -> Result<(), DecodeError>> Visitor<'de> for Items<F> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a JSON array with unique object keys")
        }
        fn visit_seq<A: SeqAccess<'de>>(mut self, mut a: A) -> Result<(), A::Error> {
            let mut index = 0;
            while let Some(Unique(value)) = a.next_element()? {
                (self.0)(index, value).map_err(de::Error::custom)?;
                index += 1;
            }
            Ok(())
        }
    }
    let mut d = serde_json::Deserializer::from_slice(bytes);
    (&mut d)
        .deserialize_seq(Items(visitor))
        .map_err(|e| DecodeError::Invalid(e.to_string()))?;
    d.end().map_err(|e| DecodeError::Invalid(e.to_string()))
}
