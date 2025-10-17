use serde::Serialize;

use crate::{
    helix_engine::types::{GraphError, VectorError},
    protocol::value::Value,
    utils::bump_vec_map::BumpVecMap,
};
use core::fmt;

use std::{borrow::Cow, fmt::Debug};

// TODO: make this generic over the type of encoding (f32, f64, etc)
// TODO: use const param to set dimension
// TODO: set level as u8

#[repr(C, align(16))]
#[derive(Clone, Serialize, PartialEq)]
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
    pub properties: Option<BumpVecMap<'arena, &'arena str, Value>>,
}

impl Debug for VectorWithoutData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ \nid: {},\nlevel: {},\nproperties: {:#?} }}",
            uuid::Uuid::from_u128(self.id),
            self.level,
            self.properties
        )
    }
}

impl<'arena> VectorWithoutData<'arena> {
    #[inline(always)]
    pub fn from_properties(
        id: u128,
        label: &'arena str,
        level: usize,
        properties: BumpVecMap<'arena, &'arena str, Value>,
    ) -> Self {
        VectorWithoutData {
            id,
            label,
            version: 1,
            level,
            properties: Some(properties),
        }
    }

    #[inline(always)]
    pub fn decode_vector(
        id: u128,
        label: &'arena str,
        properties: BumpVecMap<'arena, &'arena str, Value>,
    ) -> Result<Self, VectorError> {
        let vector = VectorWithoutData::from_properties(id, label, 0, properties);
        Ok(vector)
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

    pub fn check_property(&self, key: &str) -> Result<Cow<'_, Value>, GraphError> {
        match key {
            "id" => Ok(Cow::Owned(Value::from(self.uuid()))),
            "label" => Ok(Cow::Owned(Value::from(self.label().to_string()))),
            _ => match &self.properties {
                Some(properties) => properties
                    .get(key)
                    .ok_or(GraphError::ConversionError(format!(
                        "Property {key} not found"
                    )))
                    .map(Cow::Borrowed),
                None => Err(GraphError::ConversionError(format!(
                    "Property {key} not found"
                ))),
            },
        }
    }

    pub fn id(&self) -> &u128 {
        &self.id
    }

    pub fn uuid(&self) -> String {
        uuid::Uuid::from_u128(self.id).to_string()
    }

    pub fn label(&self) -> &'arena str {
        self.label
    }
}
