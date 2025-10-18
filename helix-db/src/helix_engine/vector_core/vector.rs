use serde::{Deserialize, Serialize};

use crate::{
    helix_engine::{
        types::{GraphError, VectorError},
        vector_core::{vector_distance::DistanceCalc, vector_without_data::VectorWithoutData},
    },
    protocol::value::Value,
    utils::{
        id::v6_uuid, properties::ImmutablePropertiesMap,
    },
};
use core::fmt;

use std::{borrow::Cow, cmp::Ordering, collections::HashMap, fmt::Debug};

// TODO: make this generic over the type of encoding (f32, f64, etc)
// TODO: use const param to set dimension
// TODO: set level as u8

#[repr(C, align(16))] // TODO: see performance impact of repr(C) and align(16)
#[derive(Clone, Serialize, Deserialize, PartialEq)]
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
            "{{ \nid: {},\nlevel: {},\ndistance: {:?},\ndata: {:?},\nproperties: {:#?} }}",
            uuid::Uuid::from_u128(self.id),
            // self.is_deleted,
            self.level,
            self.distance,
            self.data,
            self.properties
        )
    }
}

impl<'arena> HVector<'arena> {
    #[inline(always)]
    pub fn new(label: &'arena str, data: &'arena [f64]) -> Self {
        let id = v6_uuid();
        HVector {
            id,
            // is_deleted: false,
            version: 1,
            level: 0,
            label,
            data,
            distance: None,
            properties: None,
        }
    }

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

    #[inline(always)]
    pub fn decode_vector(
        raw_vector_bytes: &[u8],
        properties: Option<HashMap<String, Value>>,
        id: u128,
        label: &'arena str,
        arena: &'arena bumpalo::Bump,
    ) -> Result<Self, VectorError> {
        let mut vector = HVector::from_bytes(id, label, 0, raw_vector_bytes, arena)?;
        vector.properties = properties;
        Ok(vector)
    }

    /// Returns the data of the HVector
    #[inline(always)]
    pub fn get_data(&self) -> &[f64] {
        &self.data
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

    /// Converts the HVector to an vec of bytes by accessing the data field directly
    /// and converting each f64 to a byte slice
    pub fn to_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.data)
    }

    // will make to use const param for type of encoding (f32, f64, etc)
    /// Converts a byte array into a HVector by chunking the bytes into f64 values
    pub fn from_bytes(
        id: u128,
        label: &'arena str,
        level: usize,
        bytes: &[u8],
        arena: &'arena bumpalo::Bump,
    ) -> Result<Self, VectorError> {
        let data = bytemuck::try_cast_slice::<u8, f64>(bytes)
            .map_err(|_| VectorError::InvalidVectorData)?;

        let data = arena.alloc_slice_copy(data);

        Ok(HVector {
            id,
            // is_deleted: false,
            label,
            level,
            version: 1,
            data,
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

    pub fn get_property(&self, key: &str) -> Option<&Value> {
        match key {
            "id" => Some(&Value::from(self.uuid())),
            "label" => Some(&Value::from(self.label().to_string())),
            "data" => Some(&Value::Array(
                self.data.iter().map(|f| Value::F64(*f)).collect(),
            )),
            "score" => Some(&Value::F64(self.score())),
            _ => self.properties.as_ref().and_then(|value| value.get(key)),
        }
    }

    pub fn id(&self) -> &u128 {
        &self.id
    }

    pub fn uuid(&self) -> String {
        uuid::Uuid::from_u128(self.id).to_string()
    }

    pub fn label(&self) -> &str {
        match &self.properties {
            Some(properties) => match properties.get("label") {
                Some(label) => label.as_str(),
                None => "vector",
            },
            None => "vector",
        }
    }

    pub fn score(&self) -> f64 {
        self.distance.unwrap_or(2.0)
    }
}
