use std::collections::HashMap;

use serde::{Serialize, ser::SerializeSeq};

use crate::protocol::value::Value;

pub struct GroupBy(pub HashMap<String, HashMap<String, Value>>);

impl Serialize for GroupBy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (_, v) in &self.0 {
            seq.serialize_element(&v)?;
        }
        seq.end()
    }
}
