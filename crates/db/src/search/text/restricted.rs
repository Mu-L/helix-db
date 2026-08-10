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

    pub(crate) fn len(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::NonEmpty(bitmap) => bitmap.len(),
        }
    }

    pub(crate) fn contains(&self, entity_id: u64) -> bool {
        match self {
            Self::Empty => false,
            Self::NonEmpty(bitmap) => bitmap.contains(entity_id),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        match self {
            Self::Empty => None.into_iter().flatten(),
            Self::NonEmpty(bitmap) => Some(bitmap.iter()).into_iter().flatten(),
        }
    }
}

/// Explicit unrestricted or exact-candidate scope for every physical FTS read.
#[derive(Debug, Clone)]
pub(crate) enum TextSearchScope {
    Unrestricted,
    Restricted {
        candidates: Arc<RestrictedTextCandidates>,
        strategy: RestrictedTextStrategy,
    },
}

/// Exact physical filter implementations compared by the production benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestrictedTextStrategy {
    Adaptive,
    TermSet,
    Collector,
}

impl TextSearchScope {
    pub(crate) fn restricted(candidates: Arc<RestrictedTextCandidates>) -> Self {
        Self::Restricted {
            candidates,
            strategy: RestrictedTextStrategy::Adaptive,
        }
    }

    #[cfg(feature = "production-coverage")]
    pub(crate) fn restricted_with_strategy(
        candidates: Arc<RestrictedTextCandidates>,
        strategy: RestrictedTextStrategy,
    ) -> Self {
        Self::Restricted {
            candidates,
            strategy,
        }
    }

    pub(crate) fn candidates(&self) -> Option<&RestrictedTextCandidates> {
        match self {
            Self::Unrestricted => None,
            Self::Restricted { candidates, .. } => Some(candidates),
        }
    }

    pub(crate) fn candidate_arc(&self) -> Option<Arc<RestrictedTextCandidates>> {
        match self {
            Self::Unrestricted => None,
            Self::Restricted { candidates, .. } => Some(Arc::clone(candidates)),
        }
    }

    pub(crate) fn restricted_strategy(&self) -> Option<RestrictedTextStrategy> {
        match self {
            Self::Unrestricted => None,
            Self::Restricted { strategy, .. } => Some(*strategy),
        }
    }

    pub(crate) fn is_empty_restricted(&self) -> bool {
        self.candidates()
            .is_some_and(RestrictedTextCandidates::is_empty)
    }

    /// Applies the measured 0.1% collector crossover when corpus size is known.
    pub(crate) fn resolve_strategy(self, total_document_count: Option<u64>) -> Self {
        let Self::Restricted {
            candidates,
            strategy: RestrictedTextStrategy::Adaptive,
        } = self
        else {
            return self;
        };
        let strategy = match total_document_count {
            Some(total_document_count)
                if u128::from(candidates.len()) * 1_000 >= u128::from(total_document_count) =>
            {
                RestrictedTextStrategy::Collector
            }
            _ => RestrictedTextStrategy::TermSet,
        };
        Self::Restricted {
            candidates,
            strategy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_deduplicate_and_preserve_sorted_exact_membership() {
        let candidates = RestrictedTextCandidates::from_ids([9, 2, 9, 4]).unwrap();

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates.iter().collect::<Vec<_>>(), vec![2, 4, 9]);
        assert!(candidates.contains(4));
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

    #[test]
    fn adaptive_strategy_uses_the_measured_point_one_percent_crossover() {
        let sparse = TextSearchScope::restricted(Arc::new(
            RestrictedTextCandidates::from_ids(0..99).unwrap(),
        ))
        .resolve_strategy(Some(100_000));
        assert_eq!(
            sparse.restricted_strategy(),
            Some(RestrictedTextStrategy::TermSet)
        );

        let crossover = TextSearchScope::restricted(Arc::new(
            RestrictedTextCandidates::from_ids(0..100).unwrap(),
        ))
        .resolve_strategy(Some(100_000));
        assert_eq!(
            crossover.restricted_strategy(),
            Some(RestrictedTextStrategy::Collector)
        );
    }
}
