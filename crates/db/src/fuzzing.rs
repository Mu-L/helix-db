//! Feature-gated decoder entry points for out-of-process fuzz targets.
//!
//! The fuzz package cannot name private persisted DTOs, so this module exposes
//! only byte-slice consumers and no decoded values. Each entry point dispatches
//! into the real `encoding/v1` decoder for a closed record family. The default
//! database build omits this module entirely, and enabling it changes neither
//! key construction nor value serialization.

use crate::encoding::v1::{
    keys::tenant::{DataScope, TenantId},
    values::{secondary, text_index, vectors},
};
use crate::encoding::v2::{
    keys::{GlobalKey, Key},
    values::{
        decode_applied_state, decode_build_artifact, decode_build_delta, decode_corpus_statistics,
        decode_index_record, decode_manifest_page, decode_manifest_root, decode_metadata_value,
        decode_operation_record, decode_partition_mapping, decode_secondary_entry,
        decode_statistics_entity, decode_term_statistics, decode_text_entity_state,
    },
};

/// Exercises the complete physical framing boundary for V2 index keys.
///
/// The selector covers legacy-unscoped, tenant-scoped, and database-global
/// storage envelopes. Tenant decoding deliberately uses a fixed scope so
/// cross-scope frames reach the same production rejection path as storage
/// reads rather than a fuzz-only parser.
pub fn decode_current_index_v2_key(selector: u8, data: &[u8]) {
    match selector % 3 {
        0 => {
            let _ = Key::parse_from_slice(DataScope::LegacyUnscoped, data);
        }
        1 => {
            let scope = DataScope::Tenant(TenantId::from_u128(u128::MAX));
            let _ = Key::parse_from_slice(scope, data);
        }
        _ => {
            let _ = GlobalKey::parse_from_slice(data);
        }
    }
}

/// Exercises canonical V2 catalog, operation, and global-control values.
///
/// Inputs flow unchanged into the hand-written production codecs. Successful
/// decoding therefore proves the exact frozen V2 record framing rather than a
/// serde or harness-specific representation.
pub fn decode_current_index_v2_record(selector: u8, data: &[u8]) {
    match selector % 3 {
        0 => {
            let _ = decode_index_record(data);
        }
        1 => {
            let _ = decode_operation_record(data);
        }
        _ => {
            let _ = decode_metadata_value(data);
        }
    }
}

/// Exercises all V2 physical-work, upload, proof, reachability, and GC values.
pub fn decode_current_index_v2_work(data: &[u8]) {
    let _ = typed_work_value_is_valid(data);
}

fn typed_work_value_is_valid(data: &[u8]) -> bool {
    decode_build_delta(data).is_ok()
        || decode_applied_state(data).is_ok()
        || decode_secondary_entry(data).is_ok()
        || decode_manifest_root(data).is_ok()
        || decode_manifest_page(data).is_ok()
        || decode_build_artifact(data).is_ok()
        || decode_text_entity_state(data).is_ok()
        || decode_partition_mapping(data).is_ok()
        || decode_corpus_statistics(data).is_ok()
        || decode_term_statistics(data).is_ok()
        || decode_statistics_entity(data).is_ok()
}

/// Exercises one deployed secondary row decoder.
///
/// `selector` chooses a closed decoder family. Invalid bytes are expected to
/// return a typed error; successful decoding must satisfy the production DTO's
/// serde or structural invariants. The function intentionally returns no value
/// so the fuzz crate cannot use private persisted representations.
pub fn decode_current_secondary_record(selector: u8, data: &[u8]) {
    match selector % 2 {
        0 => {
            let _ = secondary::SecondaryRangePresence::decode(data);
        }
        _ => {
            let _ = secondary::SecondaryEqualityValue::decode(data);
        }
    }
}

/// Exercises one deployed text or vector value decoder.
///
/// The selector covers current text manifests/version/live-state rows, every
/// vector row component. Decoders receive the
/// caller's bytes unchanged; the harness never synthesizes a replacement
/// physical representation.
pub fn decode_current_search_record(selector: u8, data: &[u8]) {
    match selector % 11 {
        0 => {
            let _ = text_index::decode_manifest(data);
        }
        1 => {
            let _ = text_index::decode_version_counter(data);
        }
        2 => {
            let _ = text_index::decode_live_state(data);
        }
        3 => {
            let _ = vectors::entry::decode_entry_candidate_layer(data);
        }
        4 => {
            let _ = vectors::neighbors::decode_flat_neighbors(data);
        }
        5 => {
            let _ = vectors::neighbors::decode_upper_neighbors(data);
        }
        6 => {
            let _ = vectors::metadata::decode_metadata(data);
        }
        7 => {
            let _ = vectors::simhash::decode_simhash(data);
        }
        8 => {
            let _ = vectors::decode_layer0_neighbors_and_simhash(data);
        }
        9 => {
            let _ = vectors::markers::decode_active_txn_guard(data);
        }
        _ => {
            let header_len = data.first().copied().map_or(0, usize::from);
            let _ = vectors::item::split_item_parts(data, header_len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_seed(seed: &[u8]) -> Vec<u8> {
        let encoded = seed
            .strip_prefix(b"hex:")
            .expect("V2 golden corpus seed uses the reviewable hex envelope");
        let encoded = encoded.strip_suffix(b"\n").unwrap_or(encoded);
        assert_eq!(encoded.len() % 2, 0, "hex seed contains complete bytes");
        encoded
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                let pair = std::str::from_utf8(pair).expect("hex seed is ASCII");
                u8::from_str_radix(pair, 16).expect("hex seed contains hexadecimal bytes")
            })
            .collect()
    }

    #[test]
    fn malformed_inputs_are_total_across_every_decoder_family() {
        for selector in 0..3 {
            decode_current_index_v2_key(selector, b"malformed");
            decode_current_index_v2_record(selector, b"malformed");
        }
        decode_current_index_v2_work(b"malformed");
        for selector in 0..2 {
            decode_current_secondary_record(selector, b"malformed");
        }
        for selector in 0..11 {
            decode_current_search_record(selector, b"malformed");
        }
    }

    #[test]
    fn valid_seed_shapes_reach_current_and_additive_decoders() {
        decode_current_secondary_record(1, b"");
        decode_current_search_record(1, b"1");
        decode_current_search_record(2, br#"{"logical_version":1,"live":true}"#);
        decode_current_search_record(7, b"12345678");
    }

    #[test]
    fn checked_in_corpus_seeds_are_contract_valid() {
        let manifest = include_bytes!("../fuzz/corpus/current_search_records/text-manifest.json");
        assert!(text_index::decode_manifest(&manifest[1..manifest.len() - 1]).is_ok());
        let live = include_bytes!("../fuzz/corpus/current_search_records/text-live-state.json");
        assert!(text_index::decode_live_state(&live[1..live.len() - 1]).is_ok());
        let version = include_bytes!("../fuzz/corpus/current_search_records/text-version.json");
        assert!(text_index::decode_version_counter(&version[1..version.len() - 1]).is_ok());
        let entry = include_bytes!("../fuzz/corpus/current_search_records/vector-entry.bin");
        assert!(vectors::entry::decode_entry_candidate_layer(&entry[1..entry.len() - 1]).is_ok());
        let simhash = include_bytes!("../fuzz/corpus/current_search_records/vector-simhash.bin");
        assert!(vectors::simhash::decode_simhash(&simhash[1..simhash.len() - 1]).is_ok());
        let layer0 =
            include_bytes!("../fuzz/corpus/current_search_records/vector-layer0-empty.bin");
        assert!(vectors::decode_layer0_neighbors_and_simhash(&layer0[1..layer0.len() - 1]).is_ok());
    }

    #[test]
    fn checked_in_v2_goldens_reach_their_exact_production_decoders() {
        let unscoped = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_keys/valid-unscoped-operation"
        ));
        assert!(Key::parse_from_slice(DataScope::LegacyUnscoped, &unscoped[1..]).is_ok());

        let tenant = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_keys/valid-tenant-operation"
        ));
        let tenant_scope = DataScope::Tenant(TenantId::from_u128(u128::MAX));
        assert!(Key::parse_from_slice(tenant_scope, &tenant[1..]).is_ok());

        let global = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_keys/valid-global-storage-version"
        ));
        assert!(GlobalKey::parse_from_slice(&global[1..]).is_ok());

        let index = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_records/valid-index-record"
        ));
        assert!(decode_index_record(&index[1..]).is_ok());
        let operation = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_records/valid-operation-record"
        ));
        assert!(decode_operation_record(&operation[1..]).is_ok());
        let metadata = hex_seed(include_bytes!(
            "../fuzz/corpus/current_index_v2_records/valid-storage-version"
        ));
        assert!(decode_metadata_value(&metadata[1..]).is_ok());

        for seed in [
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-coalesced-delta").as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-upload-prepared").as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-active-mutation-proof")
                .as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-reachability-reference")
                .as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-gc-first-pass").as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-gc-second-pass").as_slice(),
            include_bytes!("../fuzz/corpus/current_index_v2_work/valid-gc-reachability-mark")
                .as_slice(),
        ] {
            assert!(typed_work_value_is_valid(&hex_seed(seed)));
        }
    }
}
