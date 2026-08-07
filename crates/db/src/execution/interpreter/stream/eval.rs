//! Runtime expression, predicate, row-property, and variable-set contracts.
//!
//! The facade keeps the interpreter-visible methods in one place while each
//! child module owns one contract family and its focused tests.

mod expr;
mod numeric;
mod params;
mod predicate;
mod property;
mod sets;

pub(in crate::execution::interpreter::stream) use property::RowValueResolver;

#[cfg(test)]
mod tests;

use super::*;
