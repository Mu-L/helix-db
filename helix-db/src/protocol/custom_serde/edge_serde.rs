use crate::utils::{
    items::Edge,
    properties::{ImmutablePropertiesMap, ImmutablePropertiesMapDeSeed},
};
use std::fmt;

/// DeserializeSeed for Node that allocates label and properties into the arena
pub struct EdgeDeSeed<'arena> {
    pub arena: &'arena bumpalo::Bump,
    pub id: u128,
}

impl<'de, 'arena> serde::de::DeserializeSeed<'de> for EdgeDeSeed<'arena> {
    type Value = Edge<'arena>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct EdgeVisitor<'arena> {
            arena: &'arena bumpalo::Bump,
            id: u128,
        }

        impl<'de, 'arena> serde::de::Visitor<'de> for EdgeVisitor<'arena> {
            type Value = Edge<'arena>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Edge")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let label_string: &'de str = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let label = self.arena.alloc_str(&label_string);

                let version: u8 = seq.next_element()?.unwrap_or(0);

                let from_node: u128 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                let to_node: u128 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;

                let properties: Option<ImmutablePropertiesMap<'arena>> =
                    seq.next_element_seed(ImmutablePropertiesMapDeSeed { arena: self.arena })?;

                Ok(Edge {
                    id: self.id,
                    label,
                    version,
                    from_node,
                    to_node,
                    properties,
                })
            }
        }

        deserializer.deserialize_struct(
            "Edge",
            &["label", "version", "from_node", "to_node", "properties"],
            EdgeVisitor {
                arena: self.arena,
                id: self.id,
            },
        )
    }
}
