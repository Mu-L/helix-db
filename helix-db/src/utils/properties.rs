use serde::Serialize;

use crate::protocol::value::Value;
use std::slice;
use std::{alloc, ptr, str};


pub struct ImmutablePropertiesMap<'arena> {
    key_lengths: &'arena [usize],
    /// `&'arena [*const 'arena str]`
    key_datas: &'arena [*const u8],
    values: &'arena [Value],
}

impl<'arena> ImmutablePropertiesMap<'arena> {
    pub fn new_in(
        len: usize,
        items: impl Iterator<Item = (&'arena str, Value)>,
        arena: &'arena bumpalo::Bump,
    ) -> Self {
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
        for (key, value) in items {
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

            ImmutablePropertiesMap {
                key_lengths: slice::from_raw_parts(key_lengths.as_ptr(), len),
                key_datas: slice::from_raw_parts(key_datas.as_ptr(), len),
                values: slice::from_raw_parts(values.as_ptr(), len),
            }
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
