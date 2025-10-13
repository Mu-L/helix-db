//! ID type for nodes and edges.
//!
//! This is a wrapper around a 128-bit UUID.
//!
//! It is used to deserialize a string UUID into a 128-bit integer so that
//! it can be serialized properly for use with LMDB.
//!
//! The ID type can be dereferenced to a 128-bit integer for use with other functions that expect a 128-bit integer.

use core::fmt;
use std::ops::Deref;

use serde::{Deserializer, Serializer, de::Visitor};
use sonic_rs::{Deserialize, Serialize};

/// A wrapper around a 128-bit UUID.
///
/// This is used to represent the ID of a node or edge.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
/// The inner ID.
pub struct ID(u128);
impl ID {
    pub fn inner(&self) -> u128 {
        self.0
    }

    pub fn stringify(&self) -> String {
        uuid::Uuid::from_u128(self.0).to_string()
    }
}

impl Serialize for ID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u128(self.0)
    }
}

struct IDVisitor;

impl<'de> Visitor<'de> for IDVisitor {
    type Value = ID;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid UUID")
    }

    /// Visits a string UUID and parses it into a 128-bit integer.
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match uuid::Uuid::parse_str(v) {
            Ok(uuid) => Ok(ID(uuid.as_u128())),
            Err(e) => Err(E::custom(e.to_string())),
        }
    }
}

/// Deserializes a string UUID into a 128-bit integer.
impl<'de> Deserialize<'de> for ID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(IDVisitor)
    }
}

/// Dereferences the ID to a 128-bit integer.
impl Deref for ID {
    type Target = u128;
    #[inline]
    fn deref(&self) -> &u128 {
        &self.0
    }
}

impl From<u128> for ID {
    fn from(id: u128) -> Self {
        ID(id)
    }
}

impl From<String> for ID {
    fn from(id: String) -> Self {
        ID(uuid::Uuid::parse_str(&id).unwrap().as_u128())
    }
}
impl From<&String> for ID {
    fn from(id: &String) -> Self {
        ID(uuid::Uuid::parse_str(id).unwrap().as_u128())
    }
}

impl From<&str> for ID {
    fn from(id: &str) -> Self {
        ID(uuid::Uuid::parse_str(id).unwrap().as_u128())
    }
}

impl From<ID> for u128 {
    fn from(id: ID) -> Self {
        id.0
    }
}

/// Generates a new v6 UUID.
///
/// This is used to generate a new UUID for a node or edge.
/// The UUID is generated using the current time and a random number.
#[inline(always)]
pub fn v6_uuid() -> u128 {
    uuid::Uuid::now_v6(&[1, 2, 3, 4, 5, 6]).as_u128()
}


#[cfg(test)]
mod tests {
    use sonic_rs::json;

    use super::*;


    #[test]
    fn test_uuid_deserialization() {
        let uuid = json!({ "id": "1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49" });

        #[derive(Deserialize)]
        struct IDWrapper {
            id: ID,
        }


        let deserialized: IDWrapper = sonic_rs::from_value(&uuid).unwrap();
        assert_eq!(deserialized.id.stringify(), "1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49");
    }

    #[test]
    fn test_uuid_serialization() {
        let uuid = "1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49";
        let id = ID::from(uuid);

        let serialized = sonic_rs::to_string(&id).unwrap();

        let uuid_u128 = str::parse::<u128>(&serialized).unwrap();
        let uuid = uuid::Uuid::from_u128(uuid_u128);

        assert_eq!(uuid.to_string(), "1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49");
    }

    // New comprehensive tests for v6_uuid() and ID type

    #[test]
    fn test_v6_uuid_generation_uniqueness() {
        let id1 = v6_uuid();
        let id2 = v6_uuid();

        // UUIDs must be unique
        assert_ne!(id1, id2, "Generated UUIDs should be unique");

        // Must be valid u128 (non-zero)
        assert!(id1 > 0, "UUID should be non-zero");
        assert!(id2 > 0, "UUID should be non-zero");
    }

    #[test]
    fn test_v6_uuid_monotonicity() {
        // UUID v6 should be time-ordered (monotonically increasing)
        let mut ids = Vec::new();
        for _ in 0..100 {
            ids.push(v6_uuid());
            // Small delay to ensure time difference
            std::thread::sleep(std::time::Duration::from_micros(1));
        }

        // Check that most IDs are monotonically increasing
        let mut increasing_count = 0;
        for window in ids.windows(2) {
            if window[0] < window[1] {
                increasing_count += 1;
            }
        }

        // At least 95% should be increasing (allowing for some edge cases)
        assert!(
            increasing_count >= 95,
            "UUID v6 should be mostly monotonically increasing. Got {}/99 increasing",
            increasing_count
        );
    }

    #[test]
    fn test_uuid_roundtrip_string() {
        let original_id = v6_uuid();
        let id = ID::from(original_id);
        let string = id.stringify();
        let parsed = ID::from(string.as_str());

        assert_eq!(*id, *parsed, "UUID roundtrip through string should preserve value");
        assert_eq!(id.inner(), parsed.inner());
    }

    #[test]
    fn test_id_from_u128() {
        let value: u128 = 12345678901234567890;
        let id = ID::from(value);

        assert_eq!(*id, value);
        assert_eq!(id.inner(), value);
    }

    #[test]
    fn test_id_from_string() {
        let uuid_str = "1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49";
        let id = ID::from(uuid_str);

        assert_eq!(id.stringify(), uuid_str);
    }

    #[test]
    fn test_id_from_string_owned() {
        let uuid_str = String::from("1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49");
        let id = ID::from(uuid_str.clone());

        assert_eq!(id.stringify(), uuid_str);
    }

    #[test]
    fn test_id_from_string_ref() {
        let uuid_str = String::from("1f07ae4b-e354-6660-b5f0-fd3ce8bc4b49");
        let id = ID::from(&uuid_str);

        assert_eq!(id.stringify(), uuid_str);
    }

    #[test]
    fn test_id_deref() {
        let value: u128 = 12345678901234567890;
        let id = ID::from(value);

        // Test Deref trait
        let deref_value: &u128 = &*id;
        assert_eq!(*deref_value, value);
    }

    #[test]
    fn test_id_into_u128() {
        let value: u128 = 12345678901234567890;
        let id = ID::from(value);
        let back: u128 = id.into();

        assert_eq!(back, value);
    }

    #[test]
    fn test_id_comparison() {
        let id1 = ID::from(100u128);
        let id2 = ID::from(200u128);
        let id3 = ID::from(100u128);

        assert!(id1 < id2);
        assert!(id2 > id1);
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_ordering() {
        let mut ids = vec![
            ID::from(300u128),
            ID::from(100u128),
            ID::from(200u128),
        ];

        ids.sort();

        assert_eq!(*ids[0], 100u128);
        assert_eq!(*ids[1], 200u128);
        assert_eq!(*ids[2], 300u128);
    }

    #[test]
    #[should_panic]
    fn test_id_from_invalid_uuid_string() {
        // This should panic because the UUID string is invalid
        let _ = ID::from("not-a-valid-uuid");
    }

    #[test]
    fn test_v6_uuid_performance() {
        // Generate 10k UUIDs and ensure it completes in reasonable time
        let start = std::time::Instant::now();
        let mut ids = Vec::with_capacity(10_000);

        for _ in 0..10_000 {
            ids.push(v6_uuid());
        }

        let elapsed = start.elapsed();

        // Should complete in less than 1 second
        assert!(elapsed.as_secs() < 1, "UUID generation too slow: {:?}", elapsed);

        // Verify all are unique
        let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, 10_000, "All generated UUIDs should be unique");
    }
}