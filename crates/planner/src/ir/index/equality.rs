use helix_ast::value::PropertyValue;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ir::NonEmptyString;

/// Invalid literal payload for a secondary index lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryIndexLiteralError {
    /// Secondary indexes do not store nested array/object values.
    NestedValue,
}

/// Literal value that can be looked up in a secondary equality index.
///
/// Secondary equality indexes share the storage-side value contract used by
/// secondary indexes. Nested heterogeneous arrays and objects are rejected.
/// Null is resolved through an authoritative scan, while `"null"` is an
/// ordinary typed string.
///
/// ```
/// use helix_ast::value::PropertyValue;
/// use helix_planner::ir::{SecondaryIndexLiteral, SecondaryIndexLiteralError};
///
/// let value = SecondaryIndexLiteral::new(PropertyValue::from("alice")).unwrap();
/// assert_eq!(
///     serde_json::to_string(&value).unwrap(),
///     r#"{"string":"alice"}"#
/// );
/// assert_eq!(
///     SecondaryIndexLiteral::new(PropertyValue::array([1])),
///     Err(SecondaryIndexLiteralError::NestedValue)
/// );
/// assert!(SecondaryIndexLiteral::new(PropertyValue::Null).is_ok());
/// assert!(SecondaryIndexLiteral::new(PropertyValue::from("null")).is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SecondaryIndexLiteral {
    value: PropertyValue,
}

impl SecondaryIndexLiteral {
    /// Build a secondary-index literal, rejecting nested array/object values.
    pub fn new(value: PropertyValue) -> Result<Self, SecondaryIndexLiteralError> {
        match value {
            PropertyValue::Array(_) | PropertyValue::Object(_) => {
                Err(SecondaryIndexLiteralError::NestedValue)
            }
            value => Ok(Self { value }),
        }
    }

    /// Borrow the validated literal value.
    ///
    /// ```
    /// use helix_ast::value::PropertyValue;
    /// use helix_planner::ir::SecondaryIndexLiteral;
    ///
    /// let literal = SecondaryIndexLiteral::new(PropertyValue::from("alice")).unwrap();
    /// assert_eq!(literal.as_property_value().as_str(), Some("alice"));
    /// ```
    pub fn as_property_value(&self) -> &PropertyValue {
        &self.value
    }
}

impl<'de> Deserialize<'de> for SecondaryIndexLiteral {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PropertyValue::deserialize(deserializer)?;
        Self::new(value).map_err(|_| D::Error::custom("expected non-nested secondary index value"))
    }
}

/// Equality-index lookup value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexValue {
    /// Literal value.
    Literal(SecondaryIndexLiteral),
    /// Runtime parameter value.
    Param(NonEmptyString),
}
