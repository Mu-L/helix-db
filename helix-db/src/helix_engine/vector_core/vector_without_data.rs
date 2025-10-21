use crate::{
    helix_engine::types::{GraphError, VectorError},
    protocol::{custom_serde::vector_serde::VectoWithoutDataDeSeed, value::Value},
    utils::properties::ImmutablePropertiesMap,
};
use bincode::Options;
use core::fmt;
use serde::Serialize;
use std::{borrow::Cow, fmt::Debug};
use uuid::Uuid;

const HYPHENATED_LENGTH: usize = 36;
// TODO: make this generic over the type of encoding (f32, f64, etc)
// TODO: use const param to set dimension
// TODO: set level as u8

#[repr(C, align(16))]
#[derive(Serialize, Clone, Copy)]
pub struct VectorWithoutData<'arena> {
    #[serde(skip)]
    /// The id of the HVector
    pub id: u128,
    /// The label of the HVector
    pub label: &'arena str,
    /// the version of the vector
    #[serde(default)]
    pub version: u8,
    /// The level of the HVector
    #[serde(skip)]
    pub level: usize,

    /// The properties of the HVector
    #[serde(default)]
    pub properties: Option<ImmutablePropertiesMap<'arena>>,
}

impl Debug for VectorWithoutData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ \nid: {},\nlevel: {} }}",
            uuid::Uuid::from_u128(self.id),
            self.level,
        )
    }
}

impl<'arena> VectorWithoutData<'arena> {
    #[inline(always)]
    pub fn from_properties(
        id: u128,
        label: &'arena str,
        level: usize,
        properties: ImmutablePropertiesMap<'arena>,
    ) -> Self {
        VectorWithoutData {
            id,
            label,
            version: 1,
            level,
            properties: Some(properties),
        }
    }

    pub fn from_bincode_bytes<'txn>(
        arena: &'arena bumpalo::Bump,
        properties: &'txn [u8],
        id: u128,
    ) -> Result<Self, VectorError> {
        bincode::options()
            .deserialize_seed(VectoWithoutDataDeSeed { arena, id }, properties)
            .map_err(|e| VectorError::ConversionError(format!("Error deserializing vector: {e}")))
    }

    /// Returns the id of the HVector
    #[inline(always)]
    pub fn get_id(&self) -> u128 {
        self.id
    }

    /// Returns the level of the HVector
    #[inline(always)]
    pub fn get_level(&self) -> usize {
        self.level
    }

    #[inline(always)]
    pub fn get_label(&self) -> &'arena str {
        self.label
    }

    #[inline(always)]
    pub fn get_property(&self, key: &str) -> Option<&'arena Value> {
        self.properties.as_ref().and_then(|value| value.get(key))
    }

    pub fn id(&self) -> &u128 {
        &self.id
    }

    pub fn label(&self) -> &'arena str {
        self.label
    }
}

impl PartialEq for VectorWithoutData<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for VectorWithoutData<'_> {}
