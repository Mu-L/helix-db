use std::fmt;
use crate::utils::{
    items::Node,
    properties::{ImmutablePropertiesMap, ImmutablePropertiesMapDeSeed},
};

/// DeserializeSeed for Node that allocates label and properties into the arena
pub struct NodeDeSeed<'arena> {
    pub arena: &'arena bumpalo::Bump,
    pub id: u128,
}

impl<'de, 'arena> serde::de::DeserializeSeed<'de> for NodeDeSeed<'arena> {
    type Value = Node<'arena>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct NodeVisitor<'arena> {
            arena: &'arena bumpalo::Bump,
            id: u128,
        }

        impl<'de, 'arena> serde::de::Visitor<'de> for NodeVisitor<'arena> {
            type Value = Node<'arena>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Node")
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

                let properties: Option<ImmutablePropertiesMap<'arena>> =
                    seq.next_element_seed(ImmutablePropertiesMapDeSeed { arena: self.arena })?;

                Ok(Node {
                    id: self.id,
                    label,
                    version,
                    properties,
                })
            }
        }

        deserializer.deserialize_struct(
            "Node",
            &["label", "version", "properties"],
            NodeVisitor {
                arena: self.arena,
                id: self.id,
            },
        )
    }
}
