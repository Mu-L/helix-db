//! A simple map implementation using a bump-allocated vector of key-value pairs.
//!
//! This provides O(n) lookups but avoids the hashing overhead of HashMap,
//! which can be faster for small maps (typically < 10-20 entries).
//! Crucially, this avoids allocation/deallocation overhead during deserialization.

use bumpalo::collections::Vec as BumpVec;
use serde::{Serialize, Serializer};
use std::fmt;

/// A map backed by a bump-allocated vector of key-value pairs.
///
/// Operations are O(n) but avoid HashMap's hashing overhead.
/// Best for small maps where you want to minimize deserialization time.
/// All data is allocated in the provided arena, avoiding heap allocations.
#[derive(Clone, PartialEq)]
pub struct BumpVecMap<'arena, K, V> {
    inner: BumpVec<'arena, (K, V)>,
}

impl<'arena, K, V> BumpVecMap<'arena, K, V> {
    /// Creates a new empty BumpVecMap in the given arena.
    #[inline]
    pub fn new_in(arena: &'arena bumpalo::Bump) -> Self {
        Self {
            inner: BumpVec::new_in(arena),
        }
    }

    /// Creates a new BumpVecMap with the specified capacity in the given arena.
    #[inline]
    pub fn with_capacity_in(capacity: usize, arena: &'arena bumpalo::Bump) -> Self {
        Self {
            inner: BumpVec::with_capacity_in(capacity, arena),
        }
    }

    /// Returns the number of elements in the map.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the map contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns an iterator over the key-value pairs.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.inner.iter()
    }

    /// Clears the map, removing all key-value pairs.
    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Creates a BumpVecMap from an iterator, allocating in the given arena.
    #[inline]
    pub fn from_iter_in<I>(iter: I, arena: &'arena bumpalo::Bump) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut map = Self::new_in(arena);
        for (k, v) in iter {
            map.inner.push((k, v));
        }
        map
    }
}

impl<'arena, K, V> BumpVecMap<'arena, K, V> {
    /// Gets a reference to the value associated with the key using any type that can be compared.
    ///
    /// O(n) operation - performs linear search.
    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + ?Sized,
    {
        self.inner
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(_, v)| v)
    }
}

impl<'arena, K: PartialEq, V> BumpVecMap<'arena, K, V> {

    /// Gets a mutable reference to the value associated with the key.
    ///
    /// O(n) operation - performs linear search.
    #[inline]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the key already exists, the old value is replaced and returned.
    /// O(n) operation - performs linear search.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some((_, v)) = self.inner.iter_mut().find(|(k, _)| k == &key) {
            Some(std::mem::replace(v, value))
        } else {
            self.inner.push((key, value));
            None
        }
    }

    /// Removes a key from the map, returning the value if it existed.
    ///
    /// O(n) operation - performs linear search.
    #[inline]
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(pos) = self.inner.iter().position(|(k, _)| k == key) {
            Some(self.inner.remove(pos).1)
        } else {
            None
        }
    }

    /// Returns true if the map contains a value for the specified key.
    ///
    /// O(n) operation - performs linear search.
    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.iter().any(|(k, _)| k == key)
    }
}

impl<'arena, K: fmt::Debug, V: fmt::Debug> fmt::Debug for BumpVecMap<'arena, K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.inner.iter().map(|(k, v)| (k, v)))
            .finish()
    }
}

// IntoIterator implementation
impl<'arena, K, V> IntoIterator for BumpVecMap<'arena, K, V> {
    type Item = (K, V);
    type IntoIter = bumpalo::collections::vec::IntoIter<'arena, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

// Serialize as a map (identical to HashMap serialization)
impl<'arena, K: Serialize, V: Serialize> Serialize for BumpVecMap<'arena, K, V> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.inner.len()))?;
        for (k, v) in &self.inner {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

// Deserialize from a map using DeserializeSeed to pass arena context
pub struct BumpVecMapSeed<'arena, K, V> {
    arena: &'arena bumpalo::Bump,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<'arena, K, V> BumpVecMapSeed<'arena, K, V> {
    pub fn new(arena: &'arena bumpalo::Bump) -> Self {
        Self {
            arena,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'de, 'arena, K, V> serde::de::DeserializeSeed<'de> for BumpVecMapSeed<'arena, K, V>
where
    K: serde::Deserialize<'de> + 'arena,
    V: serde::Deserialize<'de> + 'arena,
{
    type Value = BumpVecMap<'arena, K, V>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct BumpVecMapVisitor<'arena, K, V> {
            arena: &'arena bumpalo::Bump,
            _phantom: std::marker::PhantomData<(K, V)>,
        }

        impl<'de, 'arena, K, V> serde::de::Visitor<'de> for BumpVecMapVisitor<'arena, K, V>
        where
            K: serde::Deserialize<'de> + 'arena,
            V: serde::Deserialize<'de> + 'arena,
        {
            type Value = BumpVecMap<'arena, K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut bump_vec = BumpVec::new_in(self.arena);
                while let Some((key, value)) = map.next_entry()? {
                    bump_vec.push((key, value));
                }
                Ok(BumpVecMap { inner: bump_vec })
            }
        }

        let visitor = BumpVecMapVisitor {
            arena: self.arena,
            _phantom: std::marker::PhantomData,
        };
        deserializer.deserialize_map(visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let arena = bumpalo::Bump::new();
        let mut map = BumpVecMap::new_in(&arena);

        assert_eq!(map.len(), 0);
        assert!(map.is_empty());

        map.insert("key1", 100);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&"key1"), Some(&100));

        map.insert("key2", 200);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&"key2"), Some(&200));

        // Update existing key
        let old = map.insert("key1", 150);
        assert_eq!(old, Some(100));
        assert_eq!(map.get(&"key1"), Some(&150));
        assert_eq!(map.len(), 2);

        // Remove
        let removed = map.remove(&"key1");
        assert_eq!(removed, Some(150));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&"key1"), None);
    }

    #[test]
    fn test_contains_key() {
        let arena = bumpalo::Bump::new();
        let mut map = BumpVecMap::new_in(&arena);

        assert!(!map.contains_key(&"key1"));
        map.insert("key1", 100);
        assert!(map.contains_key(&"key1"));
        map.remove(&"key1");
        assert!(!map.contains_key(&"key1"));
    }
}
