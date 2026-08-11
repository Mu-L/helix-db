//! Exact traversal candidate membership for full-text search.

use std::sync::Arc;

use roaring::RoaringTreemap;

use crate::error::HelixDbError;

const MAX_RESTRICTED_CANDIDATES: u64 = 1_000_000;

/// Deduplicated, bounded candidate IDs supplied by an upstream traversal.
#[derive(Debug, Clone)]
pub(crate) enum RestrictedTextCandidates {
    /// The traversal produced no unique IDs.
    Empty,
    /// The traversal produced a non-empty exact bitmap.
    NonEmpty(RoaringTreemap),
}

impl RestrictedTextCandidates {
    /// Canonicalizes duplicate traversal rows into one exact bounded bitmap.
    pub(crate) fn from_ids(ids: impl IntoIterator<Item = u64>) -> Result<Self, HelixDbError> {
        let mut bitmap = RoaringTreemap::new();
        for id in ids {
            bitmap.insert(id);
            if bitmap.len() > MAX_RESTRICTED_CANDIDATES {
                return Err(HelixDbError::Query(format!(
                    "restricted text search accepts at most {MAX_RESTRICTED_CANDIDATES} unique candidates"
                )));
            }
        }
        if bitmap.is_empty() {
            Ok(Self::Empty)
        } else {
            Ok(Self::NonEmpty(bitmap))
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub(crate) fn contains(&self, entity_id: u64) -> bool {
        match self {
            Self::Empty => false,
            Self::NonEmpty(bitmap) => bitmap.contains(entity_id),
        }
    }
}

/// Explicit unrestricted or exact-candidate scope for every physical FTS read.
#[derive(Debug, Clone)]
pub(crate) enum TextSearchScope {
    Unrestricted,
    Restricted(Arc<RestrictedTextCandidates>),
}

impl TextSearchScope {
    pub(crate) fn restricted(candidates: Arc<RestrictedTextCandidates>) -> Self {
        Self::Restricted(candidates)
    }

    pub(crate) fn candidates(&self) -> Option<&RestrictedTextCandidates> {
        match self {
            Self::Unrestricted => None,
            Self::Restricted(candidates) => Some(candidates),
        }
    }

    pub(crate) fn is_empty_restricted(&self) -> bool {
        self.candidates()
            .is_some_and(RestrictedTextCandidates::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_deduplicate_and_preserve_exact_membership() {
        let candidates = RestrictedTextCandidates::from_ids([9, 2, 9, 4]).unwrap();

        let RestrictedTextCandidates::NonEmpty(bitmap) = &candidates else {
            panic!("non-empty candidate input must produce a non-empty bitmap");
        };
        assert_eq!(bitmap.len(), 3);
        assert!(candidates.contains(2));
        assert!(candidates.contains(4));
        assert!(candidates.contains(9));
        assert!(!candidates.contains(5));
    }

    #[test]
    fn candidates_represent_empty_and_reject_unique_id_overflow() {
        let empty = RestrictedTextCandidates::from_ids([]).unwrap();
        assert!(empty.is_empty());

        let error = RestrictedTextCandidates::from_ids(0..=MAX_RESTRICTED_CANDIDATES)
            .expect_err("the unique candidate cap must fail closed");
        assert!(error.to_string().contains("at most 1000000"));
    }
}
