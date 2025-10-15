use serde::Serialize;

use crate::{
    helix_engine::{
        types::{GraphError, VectorError},
        vector_core::arena::vector_distance::DistanceCalc,
    },
    protocol::{return_values::ReturnValue, value::Value},
    utils::{
        filterable::{Filterable, FilterableType},
        id::v6_uuid,
    },
};
use core::fmt;

use std::{borrow::Cow, cmp::Ordering, collections::HashMap, fmt::Debug};

// TODO: make this generic over the type of encoding (f32, f64, etc)
// TODO: use const param to set dimension
// TODO: set level as u8

#[repr(C, align(16))] // TODO: see performance impact of repr(C) and align(16)
#[derive(Clone, Serialize, PartialEq)]
pub struct HVector<'arena> {
    /// The id of the HVector
    pub id: u128,
    /// Whether the HVector is deleted (will be used for soft deletes)
    // pub is_deleted: bool,
    /// The level of the HVector
    #[serde(default)]
    pub level: usize,
    /// The distance of the HVector
    #[serde(default)]
    pub distance: Option<f64>,
    /// The actual vector
    #[serde(default)]
    pub data: bumpalo::collections::Vec<'arena, f64>,
    /// The properties of the HVector
    #[serde(default)]
    pub properties: Option<HashMap<String, Value>>,

    /// the version of the vector
    #[serde(default)]
    pub version: u8,
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
    pub fn new(data: bumpalo::collections::Vec<'arena, f64>) -> Self {
        let id = v6_uuid();
        HVector {
            id,
            // is_deleted: false,
            version: 1,
            level: 0,
            data,
            distance: None,
            properties: None,
        }
    }

    #[inline(always)]
    pub fn from_slice(level: usize, data: bumpalo::collections::Vec<'arena, f64>) -> Self {
        let id = v6_uuid();
        HVector {
            id,
            // is_deleted: false,
            version: 1,
            level,
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
        arena: &'arena bumpalo::Bump,
    ) -> Result<Self, VectorError> {
        let mut vector = HVector::from_bytes(id, 0, raw_vector_bytes, arena)?;
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
        level: usize,
        bytes: &[u8],
        arena: &'arena bumpalo::Bump,
    ) -> Result<Self, VectorError> {
        let data = bytemuck::try_cast_slice::<u8, f64>(bytes)
            .map_err(|_| VectorError::InvalidVectorData)?;

        let mut new_vec = bumpalo::collections::Vec::with_capacity_in(data.len(), arena);
        new_vec.extend_from_slice(data);

        Ok(HVector {
            id,
            // is_deleted: false,
            level,
            version: 1,
            data: new_vec,
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
        HVector::distance(self, other)
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
}

/// Filterable implementation for HVector
///
/// see helix_db/src/protocol/filterable.rs
///
/// NOTE: This could be moved to the protocol module with the node and edges in `helix_db/protocol/items.rs``
impl Filterable for HVector<'_> {
    fn type_name(&self) -> FilterableType {
        FilterableType::Vector
    }

    fn id(&self) -> &u128 {
        &self.id
    }

    fn uuid(&self) -> String {
        uuid::Uuid::from_u128(self.id).to_string()
    }

    fn label(&self) -> &str {
        match &self.properties {
            Some(properties) => match properties.get("label") {
                Some(label) => label.as_str(),
                None => "vector",
            },
            None => "vector",
        }
    }

    fn from_node(&self) -> u128 {
        unreachable!()
    }

    fn from_node_uuid(&self) -> String {
        unreachable!()
    }

    fn to_node(&self) -> u128 {
        unreachable!()
    }

    fn to_node_uuid(&self) -> String {
        unreachable!()
    }

    fn properties(self) -> Option<HashMap<String, Value>> {
        let mut properties = self.properties.unwrap_or_default();
        properties.insert(
            "data".to_string(),
            Value::Array(self.data.iter().map(|f| Value::F64(*f)).collect()),
        );
        Some(properties)
    }

    fn vector_data(&self) -> &[f64] {
        &self.data
    }

    fn score(&self) -> f64 {
        self.get_distance()
    }

    fn properties_mut(&mut self) -> &mut Option<HashMap<String, Value>> {
        &mut self.properties
    }

    fn properties_ref(&self) -> &Option<HashMap<String, Value>> {
        &self.properties
    }

    fn check_property(&self, key: &str) -> Result<Cow<'_, Value>, GraphError> {
        match key {
            "id" => Ok(Cow::Owned(Value::from(self.uuid()))),
            "label" => Ok(Cow::Owned(Value::from(self.label().to_string()))),
            "data" => Ok(Cow::Owned(Value::Array(
                self.data.iter().map(|f| Value::F64(*f)).collect(),
            ))),
            "score" => Ok(Cow::Owned(Value::F64(self.score()))),
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

    fn find_property(
        &self,
        _key: &str,
        _secondary_properties: &HashMap<String, ReturnValue>,
        _property: &mut ReturnValue,
    ) -> Option<&ReturnValue> {
        unreachable!()
    }
}
