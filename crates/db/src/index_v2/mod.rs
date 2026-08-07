//! Canonical V2 index lifecycle contracts.
//!
//! This module owns the validated logical model that the V2 key/value codecs,
//! catalog loader, DDL repository, and outbox worker share. Runtime index
//! configuration is an adapter into this model; it is not a persistence shape.
//!
//! ```
//! use db::config::SecondaryIndexDefinition;
//! use db::index_v2::{
//!     IndexGenerationId, IndexId, IndexOperationId, IndexRecordV2,
//!     IndexRevision, PhysicalGeneration, ValidatedDynamicIndexDefinition,
//! };
//!
//! let definition = ValidatedDynamicIndexDefinition::try_from(
//!     SecondaryIndexDefinition::node_equality("User", "email").unwrap(),
//! )
//! .unwrap();
//! let operation = IndexOperationId::new_v4();
//! let record = IndexRecordV2::building(
//!     IndexId::new(1).unwrap(),
//!     definition,
//!     IndexRevision::initial(),
//!     PhysicalGeneration::Secondary {
//!         generation: IndexGenerationId::initial(),
//!     },
//!     operation,
//! )
//! .unwrap();
//! assert_eq!(record.index_id().get(), 1);
//! ```

#![deny(missing_docs)]

mod catalog;
pub(crate) mod failpoints;
pub(crate) mod graph_mutation;
pub(crate) mod lifecycle;
mod metadata;
mod model;
pub(crate) mod mutation_catalog;
mod operation;
pub(crate) mod outbox;
mod public;
pub(crate) mod repository;
mod scope_gate;
pub(crate) mod secondary;
pub(crate) mod text;
pub(crate) mod vector;
pub(crate) mod work;
pub(crate) mod worker;

pub(crate) use catalog::*;
pub(crate) use metadata::*;
pub use model::*;
pub use operation::*;
pub use public::*;
pub(crate) use scope_gate::*;
pub use work::{BlobRef, TextPartition};
