//! Graph data types and encoding

pub mod error;
pub mod v1;
pub mod v2;

pub use v2::keys::{self, indexes};
pub use v2::keys::{EdgeId, NodeId};
pub use v2::values::{self, property};
