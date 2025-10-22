use crate::{
    helix_engine::vector_core::{vector::HVector, vector_without_data::VectorWithoutData},
    utils::properties::{ImmutablePropertiesMap, ImmutablePropertiesMapDeSeed},
};
use std::fmt;

/// DeserializeSeed for Node that allocates label and properties into the arena
pub struct VectorDeSeed<'txn, 'arena> {
    pub arena: &'arena bumpalo::Bump,
    pub raw_vector_data: &'txn [u8],
    pub id: u128,
}

impl<'de, 'txn, 'arena> serde::de::DeserializeSeed<'de> for VectorDeSeed<'txn, 'arena> {
    type Value = HVector<'arena>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct VectorVisitor<'txn, 'arena> {
            arena: &'arena bumpalo::Bump,
            raw_vector_data: &'txn [u8],
            id: u128,
        }

        impl<'de, 'txn, 'arena> serde::de::Visitor<'de> for VectorVisitor<'txn, 'arena> {
            type Value = HVector<'arena>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct HVector")
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

                let deleted: bool = seq.next_element()?.unwrap_or(false);

                let properties: Option<ImmutablePropertiesMap<'arena>> =
                    seq.next_element_seed(ImmutablePropertiesMapDeSeed { arena: self.arena })?;

                let data = bytemuck::try_cast_slice::<u8, f64>(self.raw_vector_data)
                    .map_err(|_| serde::de::Error::custom("Invalid vector data"))?;
                let data = self.arena.alloc_slice_copy(data);

                Ok(HVector {
                    id: self.id,
                    label,
                    deleted,
                    version,
                    level: 0,
                    distance: None,
                    data,
                    properties,
                })
            }
        }

        deserializer.deserialize_struct(
            "HVector",
            &["label", "version", "deleted", "properties"],
            VectorVisitor {
                arena: self.arena,
                raw_vector_data: self.raw_vector_data,
                id: self.id,
            },
        )
    }
}

/// DeserializeSeed for Node that allocates label and properties into the arena
pub struct VectoWithoutDataDeSeed<'arena> {
    pub arena: &'arena bumpalo::Bump,
    pub id: u128,
}

impl<'de, 'txn, 'arena> serde::de::DeserializeSeed<'de> for VectoWithoutDataDeSeed<'arena> {
    type Value = VectorWithoutData<'arena>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct VectorVisitor<'arena> {
            arena: &'arena bumpalo::Bump,
            id: u128,
        }

        impl<'de, 'arena> serde::de::Visitor<'de> for VectorVisitor<'arena> {
            type Value = VectorWithoutData<'arena>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct VectorWithoutData")
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

                let deleted: bool = seq.next_element()?.unwrap_or(false);

                let properties: Option<ImmutablePropertiesMap<'arena>> =
                    seq.next_element_seed(ImmutablePropertiesMapDeSeed { arena: self.arena })?;

                Ok(VectorWithoutData {
                    id: self.id,
                    label,
                    version,
                    deleted,
                    level: 0,
                    properties,
                })
            }
        }

        deserializer.deserialize_struct(
            "VectorWithoutData",
            &["label", "version", "deleted", "properties"],
            VectorVisitor {
                arena: self.arena,
                id: self.id,
            },
        )
    }
}
