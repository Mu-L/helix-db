//! Stored lifecycle metadata for text indexes.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_lifecycle::work::{
    TextBuildArtifactValue, TextCorpusStatisticsValue, TextEntityStateValue, TextManifestPageValue,
    TextManifestRootValue, TextStatisticsEntityValue, TextTermStatisticsValue,
};

use super::{decode_value, encode_value, WorkValue};

macro_rules! text_codec {
    ($encode:ident, $decode:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $encode(value: &$ty) -> Bytes {
            encode_value(&WorkValue::$variant(value.clone()))
        }

        pub(crate) fn $decode(value: &[u8]) -> Result<$ty, EncodingError> {
            let WorkValue::$variant(value) = decode_value(value)? else {
                return Err(EncodingError::Custom(
                    concat!(stringify!($variant), " key contains another value kind").to_string(),
                ));
            };
            Ok(value)
        }
    };
}

text_codec!(
    encode_manifest_root,
    decode_manifest_root,
    TextManifestRoot,
    TextManifestRootValue
);
text_codec!(
    encode_manifest_page,
    decode_manifest_page,
    TextManifestPage,
    TextManifestPageValue
);
text_codec!(
    encode_build_artifact,
    decode_build_artifact,
    TextBuildArtifact,
    TextBuildArtifactValue
);
text_codec!(
    encode_entity_state,
    decode_entity_state,
    TextEntityState,
    TextEntityStateValue
);
text_codec!(
    encode_corpus_statistics,
    decode_corpus_statistics,
    TextCorpusStatistics,
    TextCorpusStatisticsValue
);
text_codec!(
    encode_term_statistics,
    decode_term_statistics,
    TextTermStatistics,
    TextTermStatisticsValue
);
text_codec!(
    encode_statistics_entity,
    decode_statistics_entity,
    TextStatisticsEntity,
    TextStatisticsEntityValue
);
