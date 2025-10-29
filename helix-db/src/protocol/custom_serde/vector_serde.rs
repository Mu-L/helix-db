use crate::{
    helix_engine::vector_core::{vector::HVector, vector_without_data::VectorWithoutData},
    utils::properties::{ImmutablePropertiesMap, ImmutablePropertiesMapDeSeed},
};
use serde::de::{DeserializeSeed, Visitor};
use std::fmt;

/// Helper DeserializeSeed for Option<ImmutablePropertiesMap>
struct OptionPropertiesMapDeSeed<'arena> {
    arena: &'arena bumpalo::Bump,
}

impl<'de, 'arena> DeserializeSeed<'de> for OptionPropertiesMapDeSeed<'arena> {
    type Value = Option<ImmutablePropertiesMap<'arena>>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OptVisitor<'arena> {
            arena: &'arena bumpalo::Bump,
        }

        impl<'de, 'arena> Visitor<'de> for OptVisitor<'arena> {
            type Value = Option<ImmutablePropertiesMap<'arena>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("Option<ImmutablePropertiesMap>")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                ImmutablePropertiesMapDeSeed { arena: self.arena }
                    .deserialize(deserializer)
                    .map(Some)
            }
        }

        deserializer.deserialize_option(OptVisitor { arena: self.arena })
    }
}

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
                let label = self.arena.alloc_str(label_string);

                let version: u8 = seq.next_element()?.unwrap_or(0);

                let deleted: bool = seq.next_element()?.unwrap_or(false);

                // Use our custom DeserializeSeed that handles the Option wrapper
                let properties: Option<ImmutablePropertiesMap<'arena>> = seq
                    .next_element_seed(OptionPropertiesMapDeSeed { arena: self.arena })?
                    .ok_or_else(|| serde::de::Error::custom("Expected properties field"))?;

                // Manually copy data to avoid alignment issues with bytemuck
                if self.raw_vector_data.is_empty() {
                    return Err(serde::de::Error::custom("raw_vector_data.len() == 0"));
                }
                if !self
                    .raw_vector_data
                    .len()
                    .is_multiple_of(std::mem::size_of::<f64>())
                {
                    return Err(serde::de::Error::custom(
                        "raw_vector_data bytes len is not a multiple of size_of::<f64>()",
                    ));
                }
                let dimensions = self.raw_vector_data.len() / std::mem::size_of::<f64>();

                let layout = std::alloc::Layout::array::<f64>(dimensions).map_err(|_| {
                    serde::de::Error::custom(
                        "vector_data array arithmetic overflow or total size exceeds isize::MAX",
                    )
                })?;

                let vector_data: std::ptr::NonNull<u8> = self.arena.alloc_layout(layout);

                let data: &'arena [f64] = unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.raw_vector_data.as_ptr(),
                        vector_data.as_ptr(),
                        self.raw_vector_data.len(),
                    );

                    let vector_data: std::ptr::NonNull<f64> = vector_data.cast();
                    std::slice::from_raw_parts(vector_data.as_ptr(), dimensions)
                };

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

impl<'de, 'arena> serde::de::DeserializeSeed<'de> for VectoWithoutDataDeSeed<'arena> {
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
                let label = self.arena.alloc_str(label_string);

                let version: u8 = seq.next_element()?.unwrap_or(0);

                let deleted: bool = seq.next_element()?.unwrap_or(false);

                // Use our custom DeserializeSeed that handles the Option wrapper
                let properties: Option<ImmutablePropertiesMap<'arena>> = seq
                    .next_element_seed(OptionPropertiesMapDeSeed { arena: self.arena })?
                    .ok_or_else(|| serde::de::Error::custom("Expected properties field"))?;

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
