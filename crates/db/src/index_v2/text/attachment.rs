//! Shared typed helpers for text build and manifest metadata.

#[cfg(any(test, feature = "production-coverage"))]
use bytes::Bytes;

use crate::encoding::v1::keys::index_v2 as index_keys;
use crate::encoding::v1::keys::tenant::DataScope;
use crate::encoding::v1::keys::{DataKeyKind, Key};
use crate::encoding::v1::values::index_v2 as index_values;
use crate::error::{HelixDbError, Result};
use crate::index_v2::work;

#[cfg(any(test, feature = "production-coverage"))]
pub(super) fn scoped_key(scope: DataScope, logical_key: index_keys::IndexV2Key) -> Bytes {
    Key::Data {
        scope,
        kind: DataKeyKind::IndexV2(logical_key),
    }
    .to_bytes()
}

/// Decodes and cross-checks one generation-owned text build artifact.
pub(super) fn decode_build_artifact(
    scope: DataScope,
    operation: &super::super::IndexOperationRecord,
    key: &[u8],
    value: &[u8],
) -> Result<(
    index_keys::TextBuildArtifactKey,
    work::TextBuildArtifactValue,
)> {
    let Key::Data {
        kind: DataKeyKind::IndexV2(index_keys::IndexV2Key::TextBuildArtifact(key)),
        ..
    } = Key::parse_from_slice(scope, key)?
    else {
        return Err(corruption(
            "text artifact prefix yielded another typed key kind",
        ));
    };
    let index_values::IndexV2WorkValue::TextBuildArtifact(artifact) =
        index_values::decode_work_value(value)?
    else {
        return Err(corruption(
            "text artifact key contains another typed value kind",
        ));
    };
    if key.root.index_id != operation.index_id()
        || key.root.generation != operation.generation()
        || key.root.partition != artifact.partition.fingerprint()
        || key.ordinal != artifact.artifact_ordinal
        || artifact.index_id != operation.index_id()
        || artifact.generation != operation.generation()
    {
        return Err(corruption(
            "text build artifact key/value ownership mismatch",
        ));
    }
    Ok((key, artifact))
}

fn corruption(reason: impl Into<String>) -> HelixDbError {
    HelixDbError::IndexCatalogCorruption(reason.into())
}
