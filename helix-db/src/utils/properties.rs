use std::collections::HashMap;
use std::convert::Infallible;
use std::slice;
use std::{alloc, fmt, iter, ptr, str};

use bincode::Options;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OldPropertiesMap(HashMap<String, Value>);

#[derive(Serialize, Deserialize)]
pub enum Value {
    Type1,
    Type2,
    Type3,
    Type4,
}

/// For every node stored that we must read, we need to deserialize the Properties map.
/// Previously, this was a HashMap encoded with bincode.
///
/// To preserve backwards compatibility, it is still stored the same.
/// However, deserialization is now optimized, along with lookup.
///
/// Before: HashMap<String, Value>.
///     - Had to allocate a String for every property
///     - Had to allocate a HashMap for every property
///     - Had to hash for every get and insert.
///
/// Now:
///     - All required space is allocated in the arena upfront
///     - Key lengths are stored packed for SIMD length check on get.
///     - Small n means O(n) is faster than O(1)
pub struct ImmutablePropertiesMap<'arena> {
    key_lengths: &'arena [usize],
    /// `&'arena [*const 'arena str]`
    key_datas: &'arena [*const u8],
    values: &'arena [Value],
}

// struct Test<'arena> {
//     len: usize,
//     key_lengths: *const usize,
//     key_datas: *const *const u8,
//     values: *const Value,
//     _phantom: marker::PhantomData<(&'arena str, &'arena Value)>,
// }

impl<'arena> ImmutablePropertiesMap<'arena> {
    pub fn new(
        len: usize,
        items: impl Iterator<Item = (&'arena str, Value)>,
        arena: &'arena bumpalo::Bump,
    ) -> Self {
        let Ok(map) = Self::new_from_try(len, items.map(Ok::<_, Infallible>), arena);
        map
    }

    pub fn from_bincode_bytes<'txn>(
        bytes: &'txn [u8],
        arena: &'arena bumpalo::Bump,
    ) -> bincode::Result<Self> {
        bincode::options().deserialize_seed(ImmutablePropertiesMapDeSeed { arena }, bytes)
    }

    pub fn new_from_try<Error>(
        len: usize,
        items: impl Iterator<Item = Result<(&'arena str, Value), Error>>,
        arena: &'arena bumpalo::Bump,
    ) -> Result<Self, Error> {
        let key_length_layout = alloc::Layout::array::<usize>(len)
            .expect("LayoutError for key_length_layout: arithmetic overflow or total size exceeds isize::MAX");
        let key_datas_layout = alloc::Layout::array::<*const u8>(len)
            .expect("LayoutError for key_datas_layout: arithmetic overflow or total size exceeds isize::MAX");
        let values_layout = alloc::Layout::array::<Value>(len).expect(
            "LayoutError for values_layout: arithmetic overflow or total size exceeds isize::MAX",
        );

        let key_lengths: ptr::NonNull<usize> = arena.alloc_layout(key_length_layout).cast();
        let key_datas: ptr::NonNull<*const u8> = arena.alloc_layout(key_datas_layout).cast();
        let values: ptr::NonNull<Value> = arena.alloc_layout(values_layout).cast();

        let mut index = 0;
        for entry in items {
            let (key, value) = entry?;
            let (key_data, key_length) = (key.as_ptr(), key.len());

            unsafe {
                // SAFETY: We assert we are in-bounds above, using an incrementing counter below.
                assert!(
                    index < len,
                    "len that was passed in was incorrect, iterator is yielding more items"
                );

                key_lengths.add(index).write(key_length);
                key_datas.add(index).write(key_data);
                values.add(index).write(value);
            }

            // SAFETY: Used for out of bounds check
            index += 1;
        }

        unsafe {
            // SAFETY: We assert that the real count is correct.
            // We could still recover by constructing slices with the real length,
            // but that means somewhere is potentially messing up and could lead to
            // data loss.
            assert_eq!(
                index, len,
                "len that was passed in was incorrect, iterator yielded less items"
            );

            Ok(ImmutablePropertiesMap {
                key_lengths: slice::from_raw_parts(key_lengths.as_ptr(), len),
                key_datas: slice::from_raw_parts(key_datas.as_ptr(), len),
                values: slice::from_raw_parts(values.as_ptr(), len),
            })
        }
    }

    pub fn get(&self, q: &str) -> Option<&'arena Value> {
        self.iter().find_map(|(k, v)| q.eq(k).then_some(v))
    }

    pub fn len(&self) -> usize {
        self.key_lengths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'arena str, &'arena Value)> {
        assert!(self.key_lengths.len() == self.key_datas.len());
        assert!(self.key_lengths.len() == self.values.len());
        assert!(self.values.len() == self.key_datas.len());

        self.key_datas
            .iter()
            .copied()
            .zip(self.key_lengths.iter().copied())
            .map(|(data, len)| unsafe {
                // SAFETY: This is an immutable struct and we deconstruct a valid &'arena str
                // on creation. This is just putting it back together, and it couldn't have
                // changed in between then.
                let bytes: &'arena [u8] = slice::from_raw_parts(data, len);
                str::from_utf8_unchecked(bytes)
            })
            .zip(self.values)
    }
}

impl<'arena> Serialize for ImmutablePropertiesMap<'arena> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.values.len()))?;

        for (key, value) in self.iter() {
            map.serialize_entry(key, value)?;
        }

        map.end()
    }
}

struct ImmutablePropertiesMapDeSeed<'arena> {
    arena: &'arena bumpalo::Bump,
}

impl<'de, 'arena> serde::de::DeserializeSeed<'de> for ImmutablePropertiesMapDeSeed<'arena> {
    type Value = ImmutablePropertiesMap<'arena>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct ImmutablePropertiesMapVisitor<'arena> {
            arena: &'arena bumpalo::Bump,
        }

        impl<'de, 'arena> serde::de::Visitor<'de> for ImmutablePropertiesMapVisitor<'arena> {
            type Value = ImmutablePropertiesMap<'arena>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let num_items = map.size_hint()
                    .expect("You shouldn't have updated bincode. In v1.3.3 a size_hint was always passed for maps");

                let entries = iter::from_fn(move || {
                    map.next_entry()
                        .map(|entry| {
                            entry.map(|(k, v)| {
                                let k: &'arena str = self.arena.alloc_str(k);
                                (k, v)
                            })
                        })
                        .transpose()
                });

                ImmutablePropertiesMap::new_from_try(num_items, entries, self.arena)
            }
        }

        let visitor = ImmutablePropertiesMapVisitor { arena: self.arena };
        deserializer.deserialize_map(visitor)
    }
}
