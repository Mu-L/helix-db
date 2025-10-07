// Copyright 2025 HelixDB Inc.
// SPDX-License-Identifier: AGPL-3.0

//! Reciprocal Rank Fusion (RRF) reranker implementation.
//!
//! RRF combines multiple ranked lists without requiring score calibration.
//! Formula: RRF_score(d) = Σ 1/(k + rank_i(d))
//! where k is typically 60 (default).

use crate::{
    helix_engine::{
        reranker::{
            errors::{RerankerError, RerankerResult},
            reranker::{update_score, Reranker},
        },
        traversal_core::traversal_value::TraversalValue,
    },
};
use std::collections::HashMap;

/// Reciprocal Rank Fusion reranker.
///
/// Combines multiple ranked lists by computing reciprocal ranks.
/// This is particularly useful for hybrid search combining BM25 and vector results.
#[derive(Debug, Clone)]
pub struct RRFReranker {
    /// The k parameter in the RRF formula (default: 60)
    k: f64,
}

impl RRFReranker {
    /// Create a new RRF reranker with default k=60.
    pub fn new() -> Self {
        Self { k: 60.0 }
    }

    /// Create a new RRF reranker with custom k value.
    ///
    /// # Arguments
    /// * `k` - The k parameter in the RRF formula. Higher values give less weight to ranking position.
    pub fn with_k(k: f64) -> RerankerResult<Self> {
        if k <= 0.0 {
            return Err(RerankerError::InvalidParameter(
                "k must be positive".to_string(),
            ));
        }
        Ok(Self { k })
    }

    /// Fuse multiple ranked lists using RRF.
    ///
    /// # Arguments
    /// * `lists` - Vector of iterators, each representing a ranked list
    /// * `k` - The k parameter for RRF formula
    ///
    /// # Returns
    /// A vector of items reranked by RRF scores
    pub fn fuse_lists<I>(lists: Vec<I>, k: f64) -> RerankerResult<Vec<TraversalValue>>
    where
        I: Iterator<Item = TraversalValue>,
    {
        if lists.is_empty() {
            return Err(RerankerError::EmptyInput);
        }

        let mut rrf_scores: HashMap<u128, f64> = HashMap::new();
        let mut items_map: HashMap<u128, TraversalValue> = HashMap::new();

        // Process each ranked list
        for list in lists {
            for (rank, item) in list.enumerate() {
                let id = match &item {
                    TraversalValue::Node(n) => n.id,
                    TraversalValue::Edge(e) => e.id,
                    TraversalValue::Vector(v) => v.id,
                    _ => continue,
                };

                // Calculate reciprocal rank: 1 / (k + rank)
                // rank starts at 0, so actual rank is rank + 1
                let rr_score = 1.0 / (k + (rank as f64) + 1.0);

                // Sum reciprocal ranks across all lists
                *rrf_scores.entry(id).or_insert(0.0) += rr_score;

                // Store the item (keep first occurrence)
                items_map.entry(id).or_insert(item);
            }
        }

        // Convert to scored items and sort by RRF score (descending)
        let mut scored_items: Vec<(u128, f64)> = rrf_scores.into_iter().collect();
        scored_items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Update scores and collect results
        let mut results = Vec::with_capacity(scored_items.len());
        for (id, score) in scored_items {
            if let Some(mut item) = items_map.remove(&id) {
                update_score(&mut item, score)?;
                results.push(item);
            }
        }

        Ok(results)
    }
}

impl Default for RRFReranker {
    fn default() -> Self {
        Self::new()
    }
}

impl Reranker for RRFReranker {
    fn rerank<I>(&self, items: I, _query: Option<&str>) -> RerankerResult<Vec<TraversalValue>>
    where
        I: Iterator<Item = TraversalValue>,
    {
        // For a single list, RRF just converts ranks to RRF scores
        let items_vec: Vec<_> = items.collect();

        if items_vec.is_empty() {
            return Err(RerankerError::EmptyInput);
        }

        let mut results = Vec::with_capacity(items_vec.len());

        for (rank, mut item) in items_vec.into_iter().enumerate() {
            // Calculate RRF score for this item based on its rank
            let rrf_score = 1.0 / (self.k + (rank as f64) + 1.0);
            update_score(&mut item, rrf_score)?;
            results.push(item);
        }

        Ok(results)
    }

    fn name(&self) -> &str {
        "RRF"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        helix_engine::vector_core::vector::HVector,
        utils::items::Node,
    };

    #[test]
    fn test_rrf_single_list() {
        let reranker = RRFReranker::new();

        let vectors: Vec<TraversalValue> = (0..5)
            .map(|i| {
                let mut v = HVector::new(vec![1.0, 2.0, 3.0]);
                v.distance = Some((i + 1) as f64);
                v.id = i as u128;
                TraversalValue::Vector(v)
            })
            .collect();

        let results = reranker.rerank(vectors.into_iter(), None).unwrap();

        assert_eq!(results.len(), 5);

        // Check that RRF scores are calculated correctly
        for (rank, item) in results.iter().enumerate() {
            if let TraversalValue::Vector(v) = item {
                let expected_score = 1.0 / (60.0 + (rank as f64) + 1.0);
                assert!((v.distance.unwrap() - expected_score).abs() < 1e-10);
            }
        }
    }

    #[test]
    fn test_rrf_custom_k() {
        let reranker = RRFReranker::with_k(10.0).unwrap();

        let vectors: Vec<TraversalValue> = (0..3)
            .map(|i| {
                let mut v = HVector::new(vec![1.0]);
                v.id = i as u128;
                TraversalValue::Vector(v)
            })
            .collect();

        let results = reranker.rerank(vectors.into_iter(), None).unwrap();

        // First item should have score 1/(10+1) = 1/11
        if let TraversalValue::Vector(v) = &results[0] {
            assert!((v.distance.unwrap() - 1.0 / 11.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_rrf_fuse_multiple_lists() {
        // Create two lists with some overlap
        let list1: Vec<TraversalValue> = vec![
            {
                let mut v = HVector::new(vec![1.0]);
                v.id = 1;
                TraversalValue::Vector(v)
            },
            {
                let mut v = HVector::new(vec![2.0]);
                v.id = 2;
                TraversalValue::Vector(v)
            },
            {
                let mut v = HVector::new(vec![3.0]);
                v.id = 3;
                TraversalValue::Vector(v)
            },
        ];

        let list2: Vec<TraversalValue> = vec![
            {
                let mut v = HVector::new(vec![2.0]);
                v.id = 2;
                TraversalValue::Vector(v)
            },
            {
                let mut v = HVector::new(vec![1.0]);
                v.id = 1;
                TraversalValue::Vector(v)
            },
            {
                let mut v = HVector::new(vec![4.0]);
                v.id = 4;
                TraversalValue::Vector(v)
            },
        ];

        let results = RRFReranker::fuse_lists(
            vec![list1.into_iter(), list2.into_iter()],
            60.0,
        )
        .unwrap();

        // Items 1 and 2 appear in both lists, so should have higher scores
        assert!(results.len() >= 2);

        // Item 2 appears as rank 1 in both lists, should be highest
        if let TraversalValue::Vector(v) = &results[0] {
            assert_eq!(v.id, 2);
        }
    }

    #[test]
    fn test_rrf_invalid_k() {
        let result = RRFReranker::with_k(-1.0);
        assert!(result.is_err());

        let result = RRFReranker::with_k(0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_rrf_empty_input() {
        let reranker = RRFReranker::new();
        let empty: Vec<TraversalValue> = vec![];
        let result = reranker.rerank(empty.into_iter(), None);
        assert!(result.is_err());
    }
}
