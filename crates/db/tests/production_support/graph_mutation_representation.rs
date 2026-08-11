//! Exact V1 graph-property representation contracts.
//!
//! This harness remains outside the measured production tree while exercising
//! every production representation branch through the feature-gated crate
//! boundary. It deliberately constructs only existing V1 values and bytes.

use std::collections::BTreeMap;

use crate::encoding::property::property_value::PropertyValue;
use crate::encoding::property::Property;
use crate::encoding::v1::keys::tenant::DataScope;
use crate::index_lifecycle::graph_mutation::{
    CanonicalPropertyRow, GraphEntity, GraphMutationTransition, PropertyEdit, PropertyEditOutcome,
};

/// Proves persisted representation identity is recursive and bit exact.
pub fn graph_mutation_representation_contracts() {
    let f64_nan = f64::from_bits(0x7ff8_0000_0000_0042);
    let other_f64_nan = f64::from_bits(f64_nan.to_bits().saturating_add(1));
    let f32_nan = f32::from_bits(0x7fc0_0042);
    let other_f32_nan = f32::from_bits(f32_nan.to_bits().saturating_add(1));

    for (left, right) in [
        (PropertyValue::Null, PropertyValue::Null),
        (PropertyValue::Bool(true), PropertyValue::Bool(true)),
        (PropertyValue::I64(7), PropertyValue::I64(7)),
        (PropertyValue::DateTime(7), PropertyValue::DateTime(7)),
        (PropertyValue::F64(f64_nan), PropertyValue::F64(f64_nan)),
        (PropertyValue::F32(f64_nan), PropertyValue::F32(f64_nan)),
        (
            PropertyValue::String("value".to_owned()),
            PropertyValue::String("value".to_owned()),
        ),
        (
            PropertyValue::Bytes(vec![1, 2]),
            PropertyValue::Bytes(vec![1, 2]),
        ),
        (
            PropertyValue::I64Array(vec![1, 2]),
            PropertyValue::I64Array(vec![1, 2]),
        ),
        (
            PropertyValue::F64Array(vec![-0.0, f64_nan]),
            PropertyValue::F64Array(vec![-0.0, f64_nan]),
        ),
        (
            PropertyValue::F32Array(vec![-0.0, f32_nan]),
            PropertyValue::F32Array(vec![-0.0, f32_nan]),
        ),
        (
            PropertyValue::StringArray(vec!["value".to_owned()]),
            PropertyValue::StringArray(vec!["value".to_owned()]),
        ),
        (
            PropertyValue::Array(vec![PropertyValue::F64(f64_nan)]),
            PropertyValue::Array(vec![PropertyValue::F64(f64_nan)]),
        ),
        (
            PropertyValue::Object(BTreeMap::from([(
                "value".to_owned(),
                PropertyValue::F64(f64_nan),
            )])),
            PropertyValue::Object(BTreeMap::from([(
                "value".to_owned(),
                PropertyValue::F64(f64_nan),
            )])),
        ),
    ] {
        assert!(left.same_v1_representation(&right));
    }

    for (left, right) in [
        (PropertyValue::Bool(true), PropertyValue::Bool(false)),
        (PropertyValue::I64(7), PropertyValue::I64(8)),
        (PropertyValue::DateTime(7), PropertyValue::DateTime(8)),
        (
            PropertyValue::F64(f64_nan),
            PropertyValue::F64(other_f64_nan),
        ),
        (PropertyValue::F32(-0.0), PropertyValue::F32(0.0)),
        (
            PropertyValue::String("left".to_owned()),
            PropertyValue::String("right".to_owned()),
        ),
        (PropertyValue::Bytes(vec![1]), PropertyValue::Bytes(vec![2])),
        (
            PropertyValue::I64Array(vec![1]),
            PropertyValue::I64Array(vec![2]),
        ),
        (
            PropertyValue::F64Array(vec![f64_nan]),
            PropertyValue::F64Array(vec![other_f64_nan]),
        ),
        (
            PropertyValue::F64Array(vec![f64_nan]),
            PropertyValue::F64Array(vec![f64_nan, f64_nan]),
        ),
        (
            PropertyValue::F32Array(vec![f32_nan]),
            PropertyValue::F32Array(vec![other_f32_nan]),
        ),
        (
            PropertyValue::F32Array(vec![f32_nan]),
            PropertyValue::F32Array(vec![f32_nan, f32_nan]),
        ),
        (
            PropertyValue::StringArray(vec!["left".to_owned()]),
            PropertyValue::StringArray(vec!["right".to_owned()]),
        ),
        (
            PropertyValue::Array(vec![PropertyValue::I64(1)]),
            PropertyValue::Array(vec![PropertyValue::I64(2)]),
        ),
        (
            PropertyValue::Array(vec![PropertyValue::I64(1)]),
            PropertyValue::Array(vec![PropertyValue::I64(1), PropertyValue::I64(2)]),
        ),
        (
            PropertyValue::Object(BTreeMap::from([("left".to_owned(), PropertyValue::I64(1))])),
            PropertyValue::Object(BTreeMap::from([(
                "right".to_owned(),
                PropertyValue::I64(1),
            )])),
        ),
        (
            PropertyValue::Object(BTreeMap::from([(
                "value".to_owned(),
                PropertyValue::I64(1),
            )])),
            PropertyValue::Object(BTreeMap::from([(
                "value".to_owned(),
                PropertyValue::I64(2),
            )])),
        ),
        (
            PropertyValue::Object(BTreeMap::from([(
                "value".to_owned(),
                PropertyValue::I64(1),
            )])),
            PropertyValue::Object(BTreeMap::new()),
        ),
        (PropertyValue::Null, PropertyValue::Bool(false)),
    ] {
        assert!(!left.same_v1_representation(&right));
    }

    let exact = Property::new("score", PropertyValue::F64(f64_nan));
    assert!(exact.same_v1_representation(&exact));
    assert!(!exact.same_v1_representation(&Property::new("other", PropertyValue::F64(f64_nan),)));
    assert!(
        !exact.same_v1_representation(&Property::new("score", PropertyValue::F64(other_f64_nan),))
    );

    let scope = DataScope::LegacyUnscoped;
    let entity = GraphEntity::node(1);
    let row = CanonicalPropertyRow::new(vec![exact.clone()]);
    assert!(matches!(
        GraphMutationTransition::edit(scope, entity, row.clone(), PropertyEdit::set(exact.clone()),),
        PropertyEditOutcome::Unchanged(_)
    ));
    assert!(matches!(
        GraphMutationTransition::edit(
            scope,
            entity,
            row,
            PropertyEdit::set(Property::new("score", PropertyValue::F64(other_f64_nan),)),
        ),
        PropertyEditOutcome::Changed(_)
    ));
}
