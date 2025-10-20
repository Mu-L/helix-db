use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::{
    helix_engine::{
        types::{GraphError, VectorError},
        vector_core::{vector_distance::DistanceCalc, vector_without_data::VectorWithoutData},
    },
    protocol::{custom_serde::vector_serde::VectorDeSeed, value::Value},
    utils::{id::v6_uuid, properties::ImmutablePropertiesMap},
};
use core::fmt;

use std::{borrow::Cow, cmp::Ordering, collections::HashMap, fmt::Debug};

// TODO: make this generic over the type of encoding (f32, f64, etc)
// TODO: use const param to set dimension
// TODO: set level as u8

#[repr(C, align(16))] // TODO: see performance impact of repr(C) and align(16)
#[derive(Serialize)]
pub struct HVector<'arena> {
    /// The id of the HVector
    #[serde(skip)]
    pub id: u128,
    /// The label of the HVector
    pub label: &'arena str,
    /// the version of the vector
    #[serde(default)]
    pub version: u8,
    /// The level of the HVector
    #[serde(skip)]
    pub level: usize,
    /// The distance of the HVector
    #[serde(skip)]
    pub distance: Option<f64>,
    /// The actual vector
    #[serde(skip)]
    pub data: &'arena [f64],
    /// The properties of the HVector
    #[serde(default)]
    pub properties: Option<ImmutablePropertiesMap<'arena>>,
}

impl PartialEq for HVector<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for HVector<'_> {}
impl PartialOrd for HVector<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HVector<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

impl Debug for HVector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ \nid: {},\nlevel: {},\ndistance: {:?},\ndata: {:?}, }}",
            uuid::Uuid::from_u128(self.id),
            // self.is_deleted,
            self.level,
            self.distance,
            self.data,
        )
    }
}

impl<'arena> HVector<'arena> {
    #[inline(always)]
    pub fn from_slice(label: &'arena str, level: usize, data: &'arena [f64]) -> Self {
        let id = v6_uuid();
        HVector {
            id,
            // is_deleted: false,
            version: 1,
            level,
            label,
            data,
            distance: None,
            properties: None,
        }
    }

    /// Converts the HVector to an vec of bytes by accessing the data field directly
    /// and converting each f64 to a byte slice
    pub fn vector_data_to_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.data)
    }

    // will make to use const param for type of encoding (f32, f64, etc)
    /// Converts a byte array into a HVector by chunking the bytes into f64 values
    pub fn from_bincode_bytes<'txn>(
        arena: &'arena bumpalo::Bump,
        properties: &'txn [u8],
        raw_vector_data: &'txn [u8],
        id: u128,
    ) -> Result<Self, VectorError> {
        bincode::options()
            .deserialize_seed(
                VectorDeSeed {
                    arena,
                    id,
                    raw_vector_data,
                },
                properties,
            )
            .map_err(|e| VectorError::ConversionError(format!("Error deserializing vector: {e}")))
    }

    pub fn from_raw_vector_data<'txn>(
        arena: &'arena bumpalo::Bump,
        raw_vector_data: &'txn [u8],
        label: &'arena str,
        id: u128,
    ) -> Result<Self, VectorError> {
        let data = bytemuck::try_cast_slice::<u8, f64>(raw_vector_data)
            .map_err(|_| VectorError::ConversionError("Invalid vector data".to_string()))?;
        let data = arena.alloc_slice_copy(data);
        Ok(HVector {
            id,
            label,
            data,
            version: 1,
            level: 0,
            distance: None,
            properties: None,
        })
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline(always)]
    pub fn distance_to(&self, other: &HVector) -> Result<f64, VectorError> {
        HVector::<'arena>::distance(self, other)
    }

    #[inline(always)]
    pub fn set_distance(&mut self, distance: f64) {
        self.distance = Some(distance);
    }

    #[inline(always)]
    pub fn get_distance(&self) -> f64 {
        self.distance.unwrap_or(2.0)
    }

    #[inline(always)]
    pub fn get_label(&self) -> Option<&Value> {
        match &self.properties {
            Some(p) => p.get("label"),
            None => None,
        }
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

    pub fn score(&self) -> f64 {
        self.distance.unwrap_or(2.0)
    }
}
