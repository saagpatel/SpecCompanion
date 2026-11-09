//! Validate original bytes before maps or untyped sections can discard keys.
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt};

pub fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    UniqueKeys::deserialize(&mut deserializer).and_then(|_| deserializer.end())
}

struct UniqueKeys;

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueKeysVisitor)
    }
}

struct UniqueKeysVisitor;

impl<'de> Visitor<'de> for UniqueKeysVisitor {
    type Value = UniqueKeys;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON with unique object keys")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<UniqueKeys, M::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            map.next_value::<UniqueKeys>()?;
        }
        Ok(UniqueKeys)
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<UniqueKeys, S::Error> {
        while seq.next_element::<UniqueKeys>()?.is_some() {}
        Ok(UniqueKeys)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<UniqueKeys, E> {
        Ok(UniqueKeys)
    }
}
