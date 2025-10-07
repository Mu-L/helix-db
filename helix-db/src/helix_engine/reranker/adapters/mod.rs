// Copyright 2025 HelixDB Inc.
// SPDX-License-Identifier: AGPL-3.0

//! Adapters for integrating rerankers with traversal iterators.

pub mod rerank_adapter;

pub use rerank_adapter::{RerankAdapter, RerankIterator};
