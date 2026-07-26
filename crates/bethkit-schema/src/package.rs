// SPDX-License-Identifier: Apache-2.0
//!
//! CBOR package and catalog encoding.

use std::collections::BTreeSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CallbackBinding, CallbackImplementation, Result, SchemaError, SchemaGame, SchemaManifest,
    SchemaNode, SchemaNodeId, SchemaNodeKind, SchemaRecord, SchemaSignature, ValidationStatus,
};

/// Magic bytes at the beginning of one `.bkschema` package.
pub const PACKAGE_MAGIC: [u8; 4] = *b"BKSC";

/// Magic bytes at the beginning of one schema catalog bundle.
pub const BUNDLE_MAGIC: [u8; 4] = *b"BKCT";

/// Current binary package format version.
pub const PACKAGE_FORMAT_VERSION: u16 = 2;

const PACKAGE_HEADER_LENGTH: usize = 48;
const BUNDLE_HEADER_LENGTH: usize = 12;

/// Safety limits applied while loading untrusted schema packages.
#[derive(Debug, Clone)]
pub struct SchemaLoadLimits {
    /// Maximum encoded package size.
    pub maximum_package_bytes: usize,
    /// Maximum number of records in one package.
    pub maximum_records: usize,
    /// Maximum total number of schema nodes.
    pub maximum_nodes: usize,
    /// Maximum schema tree depth.
    pub maximum_depth: usize,
    /// Maximum UTF-8 byte length of a schema string.
    pub maximum_string_bytes: usize,
}

impl Default for SchemaLoadLimits {
    fn default() -> Self {
        Self {
            maximum_package_bytes: 64 * 1024 * 1024,
            maximum_records: 4096,
            maximum_nodes: 1_000_000,
            maximum_depth: 128,
            maximum_string_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackagePayload {
    manifest: SchemaManifest,
    records: Vec<SchemaRecord>,
    #[serde(default)]
    callback_bindings: Vec<CallbackBinding>,
}

/// A validated, owned schema package for one game mode.
#[derive(Debug, Clone)]
pub struct SchemaPackage {
    manifest: SchemaManifest,
    records: Vec<SchemaRecord>,
    callback_bindings: Vec<CallbackBinding>,
    payload_sha256: [u8; 32],
}

impl SchemaPackage {
    /// Creates and validates a package from its manifest and records.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::InvalidGraph`] when schema identifiers,
    /// paths, decoder requirements, or validation metadata are invalid.
    pub fn new(manifest: SchemaManifest, records: Vec<SchemaRecord>) -> Result<Self> {
        Self::new_with_callbacks(manifest, records, Vec::new())
    }

    /// Creates and validates a package with executable callback bindings.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::InvalidGraph`] when schema identifiers, paths,
    /// callback bindings, decoder requirements, or validation metadata are
    /// invalid.
    pub fn new_with_callbacks(
        manifest: SchemaManifest,
        records: Vec<SchemaRecord>,
        callback_bindings: Vec<CallbackBinding>,
    ) -> Result<Self> {
        let mut package = Self {
            manifest,
            records,
            callback_bindings,
            payload_sha256: [0; 32],
        };
        package.validate(&SchemaLoadLimits::default())?;
        package.payload_sha256 = package.compute_payload_hash()?;
        Ok(package)
    }

    /// Opens a package from a file path with default limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the file cannot be read or the package is
    /// malformed, unsupported, oversized, or fails digest validation.
    pub fn open(path: &Path) -> Result<Self> {
        let bytes: Vec<u8> = fs::read(path)?;
        Self::from_bytes_with_limits(&bytes, &SchemaLoadLimits::default())
    }

    /// Decodes a package with default safety limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] for malformed or unsupported package data.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with_limits(bytes, &SchemaLoadLimits::default())
    }

    /// Decodes a package with caller-provided safety limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] for malformed or unsupported package data, a
    /// digest mismatch, or an exceeded safety limit.
    pub fn from_bytes_with_limits(bytes: &[u8], limits: &SchemaLoadLimits) -> Result<Self> {
        if bytes.len() > limits.maximum_package_bytes {
            return Err(SchemaError::LimitExceeded(format!(
                "package is {} bytes, limit is {}",
                bytes.len(),
                limits.maximum_package_bytes
            )));
        }
        if bytes.len() < PACKAGE_HEADER_LENGTH || bytes[..4] != PACKAGE_MAGIC {
            return Err(SchemaError::InvalidMagic);
        }

        let version: u16 = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != PACKAGE_FORMAT_VERSION {
            return Err(SchemaError::UnsupportedVersion(version));
        }
        let payload_length_u64: u64 = u64::from_le_bytes(
            bytes[8..16]
                .try_into()
                .expect("package header length was checked"),
        );
        let payload_length: usize = usize::try_from(payload_length_u64).map_err(|_| {
            SchemaError::LimitExceeded("payload length exceeds platform size".to_owned())
        })?;
        let expected_end: usize = PACKAGE_HEADER_LENGTH
            .checked_add(payload_length)
            .ok_or_else(|| SchemaError::LimitExceeded("payload length overflowed".to_owned()))?;
        if expected_end != bytes.len() {
            return Err(SchemaError::InvalidGraph(format!(
                "header declares {payload_length} payload bytes, actual length is {}",
                bytes.len().saturating_sub(PACKAGE_HEADER_LENGTH)
            )));
        }

        let expected_hash: [u8; 32] = bytes[16..48]
            .try_into()
            .expect("package header length was checked");
        let payload_bytes: &[u8] = &bytes[PACKAGE_HEADER_LENGTH..];
        let actual_hash: [u8; 32] = Sha256::digest(payload_bytes).into();
        if expected_hash != actual_hash {
            return Err(SchemaError::HashMismatch);
        }

        let payload: PackagePayload = ciborium::de::from_reader(Cursor::new(payload_bytes))
            .map_err(|error| SchemaError::CborDecode(error.to_string()))?;
        let package = Self {
            manifest: payload.manifest,
            records: payload.records,
            callback_bindings: payload.callback_bindings,
            payload_sha256: actual_hash,
        };
        package.validate(limits)?;
        Ok(package)
    }

    /// Encodes the package in deterministic `.bkschema` form.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if validation or CBOR encoding fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate(&SchemaLoadLimits::default())?;
        let payload: Vec<u8> = self.encode_payload()?;
        let hash: [u8; 32] = Sha256::digest(&payload).into();
        let payload_length: u64 = u64::try_from(payload.len())
            .map_err(|_| SchemaError::LimitExceeded("payload length exceeds u64".to_owned()))?;

        let mut output: Vec<u8> = Vec::with_capacity(PACKAGE_HEADER_LENGTH + payload.len());
        output.extend_from_slice(&PACKAGE_MAGIC);
        output.extend_from_slice(&PACKAGE_FORMAT_VERSION.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        output.extend_from_slice(&payload_length.to_le_bytes());
        output.extend_from_slice(&hash);
        output.extend_from_slice(&payload);
        Ok(output)
    }

    /// Returns package provenance and compatibility metadata.
    pub fn manifest(&self) -> &SchemaManifest {
        &self.manifest
    }

    /// Returns all main-record schemas in deterministic order.
    pub fn records(&self) -> &[SchemaRecord] {
        &self.records
    }

    /// Returns all exact-path callback bindings in deterministic order.
    pub fn callback_bindings(&self) -> &[CallbackBinding] {
        &self.callback_bindings
    }

    /// Returns the package payload digest.
    pub fn payload_sha256(&self) -> [u8; 32] {
        self.payload_sha256
    }

    fn encode_payload(&self) -> Result<Vec<u8>> {
        let payload = PackagePayload {
            manifest: self.manifest.clone(),
            records: self.records.clone(),
            callback_bindings: self.callback_bindings.clone(),
        };
        let mut encoded: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(&payload, &mut encoded)
            .map_err(|error| SchemaError::CborEncode(error.to_string()))?;
        Ok(encoded)
    }

    fn compute_payload_hash(&self) -> Result<[u8; 32]> {
        Ok(Sha256::digest(self.encode_payload()?).into())
    }

    fn validate(&self, limits: &SchemaLoadLimits) -> Result<()> {
        if self.manifest.format_version != PACKAGE_FORMAT_VERSION {
            return Err(SchemaError::UnsupportedVersion(
                self.manifest.format_version,
            ));
        }
        if self.manifest.validation_status == ValidationStatus::Approved
            && self.manifest.byte_coverage < 1.0
        {
            return Err(SchemaError::InvalidGraph(
                "approved packages require complete byte coverage".to_owned(),
            ));
        }
        if self.manifest.callbacks_classified != self.manifest.callbacks_total {
            return Err(SchemaError::InvalidGraph(
                "all exported callbacks must be classified".to_owned(),
            ));
        }
        let callback_count: u64 = u64::try_from(self.callback_bindings.len())
            .map_err(|_| SchemaError::LimitExceeded("callback count exceeds u64".to_owned()))?;
        if callback_count != self.manifest.callbacks_total {
            return Err(SchemaError::InvalidGraph(format!(
                "package has {callback_count} callback bindings but manifest declares {}",
                self.manifest.callbacks_total
            )));
        }
        if self.records.len() > limits.maximum_records {
            return Err(SchemaError::LimitExceeded(format!(
                "package has {} records, limit is {}",
                self.records.len(),
                limits.maximum_records
            )));
        }

        let mut signatures: BTreeSet<SchemaSignature> = BTreeSet::new();
        let mut node_ids: BTreeSet<SchemaNodeId> = BTreeSet::new();
        let mut paths: BTreeSet<String> = BTreeSet::new();
        let mut node_count: usize = 0;
        for record in &self.records {
            if !signatures.insert(record.signature) {
                return Err(SchemaError::InvalidGraph(format!(
                    "duplicate record signature {:?}",
                    record.signature.0
                )));
            }
            validate_string(&record.name, limits)?;
            validate_node(
                &record.root,
                1,
                limits,
                &mut node_count,
                &mut node_ids,
                &mut paths,
            )?;
        }
        for decoder in &self.manifest.required_decoders {
            if decoder.id.trim().is_empty() {
                return Err(SchemaError::InvalidGraph(
                    "decoder identifier must not be empty".to_owned(),
                ));
            }
            validate_string(&decoder.id, limits)?;
        }
        for handler in &self.manifest.required_handlers {
            if handler.id.trim().is_empty() || handler.minimum_version == 0 {
                return Err(SchemaError::InvalidGraph(
                    "handler identifier and version must be valid".to_owned(),
                ));
            }
            validate_string(&handler.id, limits)?;
        }
        validate_callback_bindings(
            &self.callback_bindings,
            &self.manifest.required_decoders,
            &self.manifest.required_handlers,
            limits,
        )?;
        Ok(())
    }
}

fn validate_callback_bindings(
    bindings: &[CallbackBinding],
    decoders: &[crate::DecoderRequirement],
    handlers: &[crate::HandlerRequirement],
    limits: &SchemaLoadLimits,
) -> Result<()> {
    let mut keys: BTreeSet<(&str, &str, Option<u32>)> = BTreeSet::new();
    for binding in bindings {
        validate_string(&binding.path, limits)?;
        validate_string(&binding.callback_id, limits)?;
        validate_string(&binding.implementation_fingerprint, limits)?;
        if binding.path.trim().is_empty() || binding.callback_id.trim().is_empty() {
            return Err(SchemaError::InvalidGraph(
                "callback path and identifier must not be empty".to_owned(),
            ));
        }
        if binding.implementation_fingerprint.len() != 64
            || !binding
                .implementation_fingerprint
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(SchemaError::InvalidGraph(format!(
                "callback {} at {} has an invalid implementation fingerprint",
                binding.callback_id, binding.path
            )));
        }
        if !keys.insert((&binding.path, &binding.callback_id, binding.callback_slot)) {
            return Err(SchemaError::InvalidGraph(format!(
                "duplicate callback binding {} at {}",
                binding.callback_id, binding.path
            )));
        }
        match &binding.implementation {
            CallbackImplementation::Declarative { .. }
            | CallbackImplementation::UserInterfaceOnly => {}
            CallbackImplementation::BuiltIn { operation } => {
                validate_string(&operation.id, limits)?;
                if operation.id.trim().is_empty() || operation.minimum_version == 0 {
                    return Err(SchemaError::InvalidGraph(
                        "built-in callback operation must not be empty".to_owned(),
                    ));
                }
                require_handler(
                    handlers,
                    &operation.id,
                    operation.minimum_version,
                    &binding.path,
                )?;
            }
            CallbackImplementation::PayloadDecoder {
                decoder,
                minimum_decoder_version,
            } => {
                validate_string(decoder, limits)?;
                if decoder.trim().is_empty() || *minimum_decoder_version == 0 {
                    return Err(SchemaError::InvalidGraph(
                        "custom callback decoder and version must be valid".to_owned(),
                    ));
                }
                if !decoders.iter().any(|requirement| {
                    requirement.id == *decoder
                        && requirement.minimum_version >= *minimum_decoder_version
                }) {
                    return Err(SchemaError::InvalidGraph(format!(
                        "custom callback decoder {decoder} is missing from manifest requirements"
                    )));
                }
            }
            CallbackImplementation::CustomHandler {
                handler,
                minimum_handler_version,
            } => {
                validate_string(handler, limits)?;
                if handler.trim().is_empty() || *minimum_handler_version == 0 {
                    return Err(SchemaError::InvalidGraph(
                        "custom callback handler and version must be valid".to_owned(),
                    ));
                }
                require_handler(handlers, handler, *minimum_handler_version, &binding.path)?;
            }
        }
    }
    Ok(())
}

fn require_handler(
    handlers: &[crate::HandlerRequirement],
    handler: &str,
    minimum_version: u32,
    path: &str,
) -> Result<()> {
    if handlers.iter().any(|requirement| {
        requirement.id == handler && requirement.minimum_version >= minimum_version
    }) {
        return Ok(());
    }
    Err(SchemaError::InvalidGraph(format!(
        "callback handler {handler} at {path} is missing from manifest requirements"
    )))
}

/// Encodes packages into one deterministic catalog bundle.
///
/// # Errors
///
/// Returns [`SchemaError`] if a package cannot be encoded, game modes are
/// duplicated, or the bundle length exceeds `u64`.
pub fn encode_bundle(packages: &[SchemaPackage]) -> Result<Vec<u8>> {
    let mut ordered: Vec<&SchemaPackage> = packages.iter().collect();
    ordered.sort_by_key(|package| package.manifest().game);

    let mut games: BTreeSet<SchemaGame> = BTreeSet::new();
    let mut encoded_packages: Vec<Vec<u8>> = Vec::with_capacity(ordered.len());
    for package in ordered {
        if !games.insert(package.manifest().game) {
            return Err(SchemaError::InvalidGraph(format!(
                "duplicate game package {}",
                package.manifest().game.slug()
            )));
        }
        encoded_packages.push(package.to_bytes()?);
    }

    let count: u32 = u32::try_from(encoded_packages.len())
        .map_err(|_| SchemaError::LimitExceeded("too many package entries".to_owned()))?;
    let mut output: Vec<u8> = Vec::new();
    output.extend_from_slice(&BUNDLE_MAGIC);
    output.extend_from_slice(&PACKAGE_FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&count.to_le_bytes());
    for package in encoded_packages {
        let length: u64 = u64::try_from(package.len()).map_err(|_| {
            SchemaError::LimitExceeded("encoded package length exceeds u64".to_owned())
        })?;
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&package);
    }
    Ok(output)
}

pub(crate) fn decode_bundle(bytes: &[u8], limits: &SchemaLoadLimits) -> Result<Vec<SchemaPackage>> {
    if bytes.len() < BUNDLE_HEADER_LENGTH || bytes[..4] != BUNDLE_MAGIC {
        return Err(SchemaError::InvalidMagic);
    }
    let version: u16 = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != PACKAGE_FORMAT_VERSION {
        return Err(SchemaError::UnsupportedVersion(version));
    }
    let count: u32 = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .expect("bundle header length was checked"),
    );
    let mut offset: usize = BUNDLE_HEADER_LENGTH;
    let mut packages: Vec<SchemaPackage> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let length_end: usize = offset
            .checked_add(8)
            .ok_or_else(|| SchemaError::InvalidGraph("bundle offset overflowed".to_owned()))?;
        let length_bytes: &[u8] = bytes.get(offset..length_end).ok_or_else(|| {
            SchemaError::InvalidGraph("bundle ended before package length".to_owned())
        })?;
        let length_u64: u64 = u64::from_le_bytes(
            length_bytes
                .try_into()
                .expect("package length slice is 8 bytes"),
        );
        let length: usize = usize::try_from(length_u64).map_err(|_| {
            SchemaError::LimitExceeded("bundle entry exceeds platform size".to_owned())
        })?;
        offset = length_end;
        let package_end: usize = offset
            .checked_add(length)
            .ok_or_else(|| SchemaError::InvalidGraph("bundle entry overflowed".to_owned()))?;
        let package_bytes: &[u8] = bytes.get(offset..package_end).ok_or_else(|| {
            SchemaError::InvalidGraph("bundle ended inside package entry".to_owned())
        })?;
        packages.push(SchemaPackage::from_bytes_with_limits(
            package_bytes,
            limits,
        )?);
        offset = package_end;
    }
    if offset != bytes.len() {
        return Err(SchemaError::InvalidGraph(
            "bundle has trailing bytes".to_owned(),
        ));
    }
    Ok(packages)
}

fn validate_node(
    node: &SchemaNode,
    depth: usize,
    limits: &SchemaLoadLimits,
    node_count: &mut usize,
    node_ids: &mut BTreeSet<SchemaNodeId>,
    paths: &mut BTreeSet<String>,
) -> Result<()> {
    if depth > limits.maximum_depth {
        return Err(SchemaError::LimitExceeded(format!(
            "schema depth exceeds {}",
            limits.maximum_depth
        )));
    }
    *node_count += 1;
    if *node_count > limits.maximum_nodes {
        return Err(SchemaError::LimitExceeded(format!(
            "schema node count exceeds {}",
            limits.maximum_nodes
        )));
    }
    if !node_ids.insert(node.id) {
        return Err(SchemaError::InvalidGraph(format!(
            "duplicate schema node id {}",
            node.id.0
        )));
    }
    if !paths.insert(node.path.clone()) {
        return Err(SchemaError::InvalidGraph(format!(
            "duplicate schema path {}",
            node.path
        )));
    }
    if node.path.trim().is_empty() {
        return Err(SchemaError::InvalidGraph(
            "schema node path must not be empty".to_owned(),
        ));
    }
    validate_string(&node.path, limits)?;
    validate_string(&node.name, limits)?;
    if let SchemaNodeKind::Primitive {
        primitive: crate::PrimitiveType::Float { scale, digits, .. },
    } = &node.kind
    {
        if !scale.is_finite() || *scale == 0.0 {
            return Err(SchemaError::InvalidGraph(format!(
                "float scale must be finite and non-zero at {}",
                node.path
            )));
        }
        if *digits < 0 && *digits != i32::MIN {
            return Err(SchemaError::InvalidGraph(format!(
                "float digits must be i32::MIN or non-negative at {}",
                node.path
            )));
        }
    }
    if let SchemaNodeKind::Primitive {
        primitive: crate::PrimitiveType::String { string },
    } = &node.kind
    {
        validate_string(&string.encoding, limits)?;
        if string.encoding.is_empty() {
            return Err(SchemaError::InvalidGraph(format!(
                "string encoding must not be empty at {}",
                node.path
            )));
        }
        if let Some(prefix) = string.length_prefix {
            if !matches!(prefix.width, 1 | 2 | 4) {
                return Err(SchemaError::InvalidGraph(format!(
                    "string length prefix width must be 1, 2, or 4 at {}",
                    node.path
                )));
            }
            if prefix.offset < prefix.width {
                return Err(SchemaError::InvalidGraph(format!(
                    "string length prefix offset precedes its value at {}",
                    node.path
                )));
            }
            if string.fixed_length.is_some() {
                return Err(SchemaError::InvalidGraph(format!(
                    "string cannot have both fixed length and a length prefix at {}",
                    node.path
                )));
            }
        }
    }
    if let SchemaNodeKind::Array {
        count: crate::ArrayCount::Prefixed { integer },
        ..
    } = &node.kind
    {
        if integer.signed {
            return Err(SchemaError::InvalidGraph(format!(
                "array count prefix must be unsigned at {}",
                node.path
            )));
        }
        if !matches!(integer.width, 1 | 2 | 4 | 8) {
            return Err(SchemaError::InvalidGraph(format!(
                "array count prefix width must be 1, 2, 4, or 8 at {}",
                node.path
            )));
        }
    }

    let children: Vec<&SchemaNode> = match &node.kind {
        SchemaNodeKind::Sequence { children } => children.iter().collect(),
        SchemaNodeKind::Choice { alternatives } => alternatives.iter().collect(),
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Array { element: child, .. } => vec![child],
        SchemaNodeKind::Struct { fields } => fields.iter().collect(),
        SchemaNodeKind::Union { variants, .. } => variants.iter().collect(),
        SchemaNodeKind::Primitive { .. }
        | SchemaNodeKind::Custom { .. }
        | SchemaNodeKind::Reference { .. } => Vec::new(),
    };
    for child in children {
        validate_node(child, depth + 1, limits, node_count, node_ids, paths)?;
    }
    Ok(())
}

fn validate_string(value: &str, limits: &SchemaLoadLimits) -> Result<()> {
    if value.len() > limits.maximum_string_bytes {
        return Err(SchemaError::LimitExceeded(format!(
            "string is {} bytes, limit is {}",
            value.len(),
            limits.maximum_string_bytes
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ByteOrder, IntegerType, PrimitiveType, SchemaNodeKind};

    fn test_package() -> Result<SchemaPackage> {
        SchemaPackage::new(
            SchemaManifest {
                format_version: PACKAGE_FORMAT_VERSION,
                game: SchemaGame::SkyrimSe,
                package_version: "1.0.0".to_owned(),
                source_repository: "TES5Edit/TES5Edit".to_owned(),
                source_tag: "xedit-4.1.5f".to_owned(),
                source_commit: "f5c00f3fa3ee39511185515802647246c807f759".to_owned(),
                source_archive_sha256: "00".repeat(32),
                exporter_version: "0.1.0".to_owned(),
                exporter_binary_sha256: "22".repeat(32),
                exporter_map_sha256: "23".repeat(32),
                exporter_patch_sha256: "33".repeat(32),
                exporter_build_sha256: "44".repeat(32),
                conversion_rules_sha256: "11".repeat(32),
                minimum_bethkit_version: "0.4.0".to_owned(),
                minimum_abi_version: 2,
                validation_status: ValidationStatus::Candidate,
                corpus_sha256: "22".repeat(32),
                validated_records: 0,
                byte_coverage: 0.0,
                callbacks_total: 0,
                callbacks_classified: 0,
                required_decoders: Vec::new(),
                required_handlers: Vec::new(),
            },
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(1),
                    path: "TEST.DATA".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: crate::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 4,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                },
            }],
        )
    }

    /// Verifies deterministic package encoding and decoding.
    #[test]
    fn package_round_trip_is_deterministic() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let package = test_package()?;

        // when
        let first = package.to_bytes()?;
        let decoded = SchemaPackage::from_bytes(&first)?;
        let second = decoded.to_bytes()?;

        // then
        assert_eq!(first, second);
        assert_eq!(decoded.records().len(), 1);
        Ok(())
    }

    /// Verifies package digest mismatches are rejected.
    #[test]
    fn package_rejects_modified_payload() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut bytes = test_package()?.to_bytes()?;
        let last = bytes.last_mut().expect("encoded test package is not empty");
        *last ^= 0xFF;

        // when
        let result = SchemaPackage::from_bytes(&bytes);

        // then
        assert!(matches!(result, Err(SchemaError::HashMismatch)));
        Ok(())
    }
}
