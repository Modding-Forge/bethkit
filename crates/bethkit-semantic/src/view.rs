// SPDX-License-Identifier: Apache-2.0
//!
//! Ordered schema-guided views over parsed records.

use std::borrow::Cow;
use std::collections::BTreeMap;

use bethkit_core::{FormId, Record, RecordFlags, Signature, SubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, CallbackImplementation, EvalContext, EvalValue, IntegerType,
    PrimitiveType, SchemaNode, SchemaNodeKind, SchemaRecord, StringType, UnionSelector,
};

use crate::handler::HandlerInvocationAccess;
use crate::value::float_from_raw;
use crate::{
    grammar::interpret, ByteSpan, Diagnostic, DiagnosticCode, DiagnosticSeverity, FieldOrigin,
    FieldValue, HandlerOutput, HandlerPhase, HandlerRecordContext, NamedValue, ParsedEditValue,
    Result, SemanticContext, SemanticError, SemanticLink, ValidationMode, ValidationReport,
    ValueFormat,
};

/// One decoded top-level record field.
#[derive(Debug)]
pub struct Field<'a> {
    /// Source subrecord index in the containing record's ordered payload.
    pub subrecord_index: usize,
    /// Actual nested grammar-repeat occurrences containing this subrecord.
    pub repeat_scopes: Vec<crate::RepeatScope>,
    /// Stable schema node identifier.
    pub node_id: bethkit_schema::SchemaNodeId,
    /// Stable schema path.
    pub path: String,
    /// Effective selected payload path for a dynamic union, when available.
    pub effective_path: Option<String>,
    /// Human-readable name.
    pub name: String,
    /// Source subrecord signature.
    pub subrecord_signature: Signature,
    /// Zero-based occurrence of the stable path, or signature for raw fields.
    pub occurrence: usize,
    /// Byte span inside the subrecord payload.
    pub span: ByteSpan,
    /// Field provenance.
    pub origin: FieldOrigin,
    /// Decoded field value.
    pub value: FieldValue<'a>,
}

/// Read-only semantic view over one parsed record.
pub struct RecordView<'context, 'record> {
    context: &'context SemanticContext,
    record: &'record Record,
    schema: &'context SchemaRecord,
    localized: bool,
}

#[derive(Clone, Copy)]
struct DecodeFrame<'scope, 'record> {
    offset: usize,
    source_subrecord_index: usize,
    sibling_values: &'scope [NamedValue<'record>],
    array_indices: &'scope [usize],
}

impl<'context, 'record> RecordView<'context, 'record> {
    pub(crate) fn new(
        context: &'context SemanticContext,
        record: &'record Record,
        plugin_localized: bool,
    ) -> Result<Self> {
        let schema: &SchemaRecord =
            context
                .registry()
                .get(record.header.signature)
                .ok_or_else(|| {
                    SemanticError::MissingRecordSchema(record.header.signature.to_string())
                })?;
        Ok(Self {
            context,
            record,
            schema,
            localized: plugin_localized,
        })
    }

    /// Returns the schema used by this view.
    pub fn schema(&self) -> &SchemaRecord {
        self.schema
    }

    /// Formats a decoded value with the xEdit callback bound to its exact path.
    ///
    /// The typed value remains unchanged. `None` means that no executable
    /// formatter is bound to the path.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the formatter rejects the value.
    pub fn format_value(&self, path: &str, value: &FieldValue<'_>) -> Result<Option<String>> {
        self.format_value_as(path, value, ValueFormat::Display)
    }

    /// Formats a decoded value using one explicit xEdit presentation mode.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound formatter rejects
    /// the value or returns an invalid result.
    pub fn format_value_as(
        &self,
        path: &str,
        value: &FieldValue<'_>,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        if self.uses_handler(path, "format.blueprint_component_summary") {
            let scope = self.structural_callback_scope()?;
            return self
                .context
                .format_value_as_in_scope(self.record, path, value, &scope, format);
        }
        self.context
            .format_value_as(self.record, path, value, format)
    }

    /// Formats a decoded value using its sibling-value container.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound formatter rejects
    /// the value or scope, or returns an invalid result.
    pub fn format_value_as_in_scope(
        &self,
        path: &str,
        value: &FieldValue<'_>,
        scope: &FieldValue<'_>,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        self.context
            .format_value_as_in_scope(self.record, path, value, scope, format)
    }

    /// Resolves the semantic link exposed by a decoded value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound link callback rejects
    /// the value or returns an invalid result.
    pub fn resolve_link(&self, path: &str, value: &FieldValue<'_>) -> Result<Option<SemanticLink>> {
        if self.uses_handler(path, "resolve.blueprint_component")
            || self.uses_handler(path, "resolve.local_array_element")
        {
            let scope = self.structural_callback_scope()?;
            return self
                .context
                .resolve_link_in_scope(self.record, path, value, &scope);
        }
        self.context.resolve_link(self.record, path, value)
    }

    /// Resolves a semantic link using the value's sibling-value container.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound link callback rejects
    /// the value or scope, or returns an invalid result.
    pub fn resolve_link_in_scope(
        &self,
        path: &str,
        value: &FieldValue<'_>,
        scope: &FieldValue<'_>,
    ) -> Result<Option<SemanticLink>> {
        self.context
            .resolve_link_in_scope(self.record, path, value, scope)
    }

    /// Parses edited xEdit text back to a typed value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound transform rejects
    /// the text or returns an invalid result.
    pub fn parse_edit_value(&self, path: &str, text: &str) -> Result<Option<ParsedEditValue>> {
        self.context.parse_edit_value(self.record, path, text)
    }

    /// Parses edited text using the value's sibling-value container.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound transform rejects
    /// the text or scope, or returns an invalid result.
    pub fn parse_edit_value_in_scope(
        &self,
        path: &str,
        text: &str,
        scope: &FieldValue<'_>,
    ) -> Result<Option<ParsedEditValue>> {
        self.context
            .parse_edit_value_in_scope(self.record, path, text, scope)
    }

    /// Returns whether xEdit allows a decoded value to be removed.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the bound removability callback
    /// rejects the value or returns an invalid result.
    pub fn is_removable(&self, path: &str, value: &FieldValue<'_>) -> Result<bool> {
        self.context.is_removable(self.record, path, value)
    }

    /// Returns the effective xEdit conflict priority for a decoded value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path is unknown or a bound callback
    /// rejects the value.
    pub fn conflict_priority(
        &self,
        path: &str,
        value: &FieldValue<'_>,
    ) -> Result<bethkit_schema::ConflictPriority> {
        self.context
            .conflict_priority_for_value(self.record, path, value)
    }

    /// Decodes top-level subrecords in their source order.
    ///
    /// Unknown and out-of-order known subrecords are preserved as borrowed
    /// bytes with distinct [`FieldOrigin`] values.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when subrecord parsing, conditions, primitive
    /// decoding, or a custom decoder fails.
    pub fn fields(&self) -> Result<Vec<Field<'record>>> {
        let subrecords: &'record [SubRecord] = self.record.subrecords()?;
        let grammar = interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        )?;
        let mut signature_occurrences: std::collections::BTreeMap<Signature, usize> =
            std::collections::BTreeMap::new();
        let mut path_occurrences: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut fields: Vec<Field<'record>> = Vec::with_capacity(subrecords.len());

        for (index, subrecord) in subrecords.iter().enumerate() {
            let signature_occurrence: usize = *signature_occurrences
                .entry(subrecord.signature)
                .and_modify(|value| *value += 1)
                .or_insert(0);
            let definition = grammar.assignments[index];
            match definition {
                Some(node) => {
                    let occurrence = *path_occurrences
                        .entry(node.path.clone())
                        .and_modify(|value| *value += 1)
                        .or_insert(0);
                    let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
                        unreachable!("definition was filtered to subrecord nodes");
                    };
                    let data: &'record [u8] = subrecord.as_bytes();
                    let mut field_values = BTreeMap::new();
                    let frame = DecodeFrame {
                        offset: 0,
                        source_subrecord_index: index,
                        sibling_values: &[],
                        array_indices: &[],
                    };
                    let (value, consumed, effective_path): (
                        FieldValue<'record>,
                        usize,
                        Option<String>,
                    ) = self.decode_node(payload, data, data, frame, &mut field_values)?;
                    if consumed != data.len() {
                        return Err(SemanticError::Decode {
                            path: payload.path.clone(),
                            message: format!(
                                "{} payload bytes were not consumed",
                                data.len().saturating_sub(consumed)
                            ),
                        });
                    }
                    fields.push(Field {
                        subrecord_index: index,
                        repeat_scopes: grammar.repeat_scopes[index].clone(),
                        node_id: node.id,
                        path: node.path.clone(),
                        effective_path,
                        name: node.name.clone(),
                        subrecord_signature: subrecord.signature,
                        occurrence,
                        span: ByteSpan {
                            start: 0,
                            end: data.len(),
                        },
                        origin: match &payload.kind {
                            SchemaNodeKind::Custom { .. } => FieldOrigin::CustomDecoder,
                            _ => FieldOrigin::Schema,
                        },
                        value,
                    });
                }
                None => {
                    let data: &'record [u8] = subrecord.as_bytes();
                    let declared = grammar.declared_signatures.contains(&subrecord.signature);
                    fields.push(Field {
                        subrecord_index: index,
                        repeat_scopes: grammar.repeat_scopes[index].clone(),
                        node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                        path: format!(
                            "{}.{}.{}",
                            if declared { "unmatched" } else { "unknown" },
                            subrecord.signature,
                            signature_occurrence
                        ),
                        effective_path: None,
                        name: if declared {
                            "Out-of-order known subrecord".to_owned()
                        } else {
                            "Unknown subrecord".to_owned()
                        },
                        subrecord_signature: subrecord.signature,
                        occurrence: signature_occurrence,
                        span: ByteSpan {
                            start: 0,
                            end: data.len(),
                        },
                        origin: if declared {
                            FieldOrigin::UnmatchedKnownSubrecord
                        } else {
                            FieldOrigin::UnknownSubrecord
                        },
                        value: FieldValue::Bytes(Cow::Borrowed(data)),
                    });
                }
            }
        }
        Ok(fields)
    }

    fn uses_handler(&self, path: &str, handler: &str) -> bool {
        self.context
            .registry()
            .package()
            .callback_bindings()
            .iter()
            .any(|binding| {
                binding.path == path
                    && matches!(
                        &binding.implementation,
                        CallbackImplementation::BuiltIn { operation }
                            if operation.id == handler
                    )
            })
    }

    fn structural_callback_scope(&self) -> Result<FieldValue<'record>> {
        let subrecords = self.record.subrecords()?;
        let grammar = interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        )?;
        let fields = self.fields()?;
        let mut repeated: BTreeMap<String, BTreeMap<u32, Vec<NamedValue<'record>>>> =
            BTreeMap::new();
        for (index, field) in fields.iter().enumerate() {
            let Some(scopes) = grammar.repeat_scopes.get(index) else {
                continue;
            };
            for scope in scopes {
                repeated
                    .entry(scope.path.clone())
                    .or_default()
                    .entry(scope.occurrence)
                    .or_default()
                    .push(named_from_field(field));
            }
        }

        let mut scope_values: Vec<NamedValue<'record>> = Vec::new();
        for (path, occurrences) in repeated {
            scope_values.push(NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path,
                effective_path: None,
                name: "Repeated structural scope".to_owned(),
                span: ByteSpan { start: 0, end: 0 },
                value: FieldValue::Array(
                    occurrences.into_values().map(FieldValue::Struct).collect(),
                ),
            });
        }
        scope_values.extend(fields.iter().map(named_from_field));
        Ok(FieldValue::Struct(scope_values))
    }

    /// Decodes each occurrence of a repeated structural schema node.
    ///
    /// Every returned value is a structure containing the top-level subrecords assigned
    /// to one repeat occurrence. This exposes callback sites such as a condition sequence
    /// made from CTDA and optional CIS1/CIS2 subrecords without merging their byte origins.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when grammar interpretation or payload decoding fails.
    pub fn repeated_structures(&self, path: &str) -> Result<Vec<FieldValue<'record>>> {
        let subrecords = self.record.subrecords()?;
        let grammar = interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        )?;
        let fields = self.fields()?;
        let mut occurrences: BTreeMap<u32, Vec<NamedValue<'record>>> = BTreeMap::new();

        for (index, field) in fields.into_iter().enumerate() {
            let Some(scope) = grammar.repeat_scopes.get(index).and_then(|scopes| {
                scopes.iter().find(|scope| {
                    scope.path == path
                        || path
                            .strip_suffix("/payload")
                            .is_some_and(|parent| parent == scope.path)
                })
            }) else {
                continue;
            };
            occurrences
                .entry(scope.occurrence)
                .or_default()
                .push(NamedValue {
                    node_id: field.node_id,
                    path: field.path,
                    effective_path: None,
                    name: field.name,
                    span: field.span,
                    value: field.value,
                });
        }

        Ok(occurrences.into_values().map(FieldValue::Struct).collect())
    }

    /// Formats one field inside a selected homogeneous array element.
    ///
    /// This supplies the selected array position and the complete decoded record context
    /// required by callbacks such as NAVM edge presentation.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the array, element, or field does not exist,
    /// structural decoding fails, or the bound formatter rejects the value.
    pub fn format_array_element_field_as(
        &self,
        array_path: &str,
        index: usize,
        field_path: &str,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        let scope = self.structural_callback_scope()?;
        let field = array_element_field(&scope, array_path, index, field_path)?;
        self.context.format_value_as_in_scope_with_array_indices(
            self.record,
            field_path,
            &field.value,
            &scope,
            &[index],
            format,
        )
    }

    /// Resolves one linked field inside a selected homogeneous array element.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the array, element, or field does not exist,
    /// structural decoding fails, or the bound link handler rejects the value.
    pub fn resolve_array_element_field_link(
        &self,
        array_path: &str,
        index: usize,
        field_path: &str,
    ) -> Result<Option<SemanticLink>> {
        let scope = self.structural_callback_scope()?;
        let field = array_element_field(&scope, array_path, index, field_path)?;
        self.context.resolve_link_in_scope_with_array_indices(
            self.record,
            field_path,
            &field.value,
            &scope,
            &[index],
        )
    }

    /// Formats one occurrence of a repeated structural callback site.
    ///
    /// `None` means that no executable value transform is bound to `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the occurrence does not exist, structural decoding
    /// fails, or the bound formatter rejects the aggregate value.
    pub fn format_repeated_structure_as(
        &self,
        path: &str,
        occurrence: usize,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        let structures = self.repeated_structures(path)?;
        let value = structures
            .get(occurrence)
            .ok_or_else(|| SemanticError::Decode {
                path: path.to_owned(),
                message: format!("repeat occurrence {occurrence} does not exist"),
            })?;
        let mut prepared = self.prepare_repeated_summary(value, value)?;
        if let FieldValue::Struct(fields) = &mut prepared {
            fields.push(repeat_position_value(occurrence, structures.len()));
        }
        self.format_value_as(path, &prepared, format)
    }

    /// Formats one field inside a selected repeated structural occurrence.
    ///
    /// This supplies both the active repeat occurrence and the complete decoded record
    /// context required by callbacks whose result depends on optional local siblings.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the occurrence or field does not exist, structural
    /// decoding fails, or the bound formatter rejects the value.
    pub fn format_repeated_field_as(
        &self,
        structure_path: &str,
        occurrence: usize,
        field_path: &str,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        let structures = self.repeated_structures(structure_path)?;
        let active = structures
            .get(occurrence)
            .ok_or_else(|| SemanticError::Decode {
                path: structure_path.to_owned(),
                message: format!("repeat occurrence {occurrence} does not exist"),
            })?;
        let field = find_named_value(active, field_path).ok_or_else(|| SemanticError::Decode {
            path: field_path.to_owned(),
            message: "field does not exist in the selected repeat occurrence".to_owned(),
        })?;
        let scope = self.repeated_field_scope(active)?;
        self.context
            .format_value_as_in_scope(self.record, field_path, &field.value, &scope, format)
    }

    /// Resolves one link inside a selected repeated structural occurrence.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the occurrence or field does not exist, structural
    /// decoding fails, or the bound link handler rejects the value.
    pub fn resolve_repeated_field_link(
        &self,
        structure_path: &str,
        occurrence: usize,
        field_path: &str,
    ) -> Result<Option<SemanticLink>> {
        let structures = self.repeated_structures(structure_path)?;
        let active = structures
            .get(occurrence)
            .ok_or_else(|| SemanticError::Decode {
                path: structure_path.to_owned(),
                message: format!("repeat occurrence {occurrence} does not exist"),
            })?;
        let field = find_named_value(active, field_path).ok_or_else(|| SemanticError::Decode {
            path: field_path.to_owned(),
            message: "field does not exist in the selected repeat occurrence".to_owned(),
        })?;
        let scope = self.repeated_field_scope(active)?;
        self.context
            .resolve_link_in_scope(self.record, field_path, &field.value, &scope)
    }

    fn repeated_field_scope(&self, active: &FieldValue<'record>) -> Result<FieldValue<'record>> {
        Ok(FieldValue::Struct(vec![
            NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path: String::new(),
                effective_path: None,
                name: "Bethkit Active Repeat Occurrence".to_owned(),
                span: ByteSpan { start: 0, end: 0 },
                value: active.clone(),
            },
            NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path: String::new(),
                effective_path: None,
                name: "Bethkit Structural Record Scope".to_owned(),
                span: ByteSpan { start: 0, end: 0 },
                value: self.structural_callback_scope()?,
            },
        ]))
    }

    fn prepare_repeated_summary(
        &self,
        value: &FieldValue<'_>,
        root_scope: &FieldValue<'_>,
    ) -> Result<FieldValue<'static>> {
        match value {
            FieldValue::Struct(fields) => {
                let mut prepared = Vec::with_capacity(fields.len());
                for field in fields {
                    let mut child = self.prepare_repeated_summary(&field.value, root_scope)?;
                    if !matches!(child, FieldValue::Struct(_) | FieldValue::Array(_))
                        && field.name != "Type"
                    {
                        let selected_path = field.effective_path.as_deref().unwrap_or(&field.path);
                        let mut formatted = self.format_value_as_in_scope(
                            selected_path,
                            &field.value,
                            root_scope,
                            ValueFormat::Summary,
                        )?;
                        if formatted.is_none() && selected_path != field.path {
                            formatted = self.format_value_as_in_scope(
                                &field.path,
                                &field.value,
                                root_scope,
                                ValueFormat::Summary,
                            )?;
                        }
                        if let Some(text) = formatted {
                            child = FieldValue::String(Cow::Owned(text));
                        }
                    }
                    prepared.push(NamedValue {
                        node_id: field.node_id,
                        path: field.path.clone(),
                        effective_path: field.effective_path.clone(),
                        name: field.name.clone(),
                        span: field.span,
                        value: child,
                    });
                }
                Ok(FieldValue::Struct(prepared))
            }
            FieldValue::Array(values) => values
                .iter()
                .map(|value| self.prepare_repeated_summary(value, root_scope))
                .collect::<Result<Vec<_>>>()
                .map(FieldValue::Array),
            _ => Ok(value.to_handler_value()),
        }
    }

    /// Validates required fields, duplicate constraints, unknown subrecords,
    /// payload decoding, and complete byte coverage.
    pub fn validate(&self) -> ValidationReport {
        self.validate_with_mode(ValidationMode::Strict)
    }

    /// Validates the record using the selected compatibility policy.
    ///
    /// [`ValidationMode::XEditCompatible`] reports missing required fields as
    /// warnings because xEdit materializes those fields while loading. Other
    /// validation diagnostics retain their strict severity.
    pub fn validate_with_mode(&self, mode: ValidationMode) -> ValidationReport {
        let mut report = ValidationReport::new();
        let subrecords: &[SubRecord] = match self.record.subrecords() {
            Ok(value) => value,
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
                return report;
            }
        };

        let grammar = match interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        ) {
            Ok(value) => value,
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
                return report;
            }
        };
        let assignments: Vec<&SchemaNode> = grammar.assignments.iter().flatten().copied().collect();
        let missing_required_severity = match mode {
            ValidationMode::Strict => DiagnosticSeverity::Error,
            ValidationMode::XEditCompatible => DiagnosticSeverity::Warning,
        };
        if !self.record.header.flags.contains(RecordFlags::DELETED) {
            for definition in missing_required_subrecords(&self.schema.root, &assignments) {
                report.push(self.diagnostic(
                    missing_required_severity,
                    DiagnosticCode::MissingRequired,
                    format!("required field {} is absent", definition.path),
                    Some(definition),
                    None,
                ));
            }
            for violation in &grammar.violations {
                report.push(self.diagnostic(
                    missing_required_severity,
                    DiagnosticCode::MissingRequired,
                    format!(
                        "repeat {} requires at least {} entries, found {}",
                        violation.path, violation.minimum, violation.actual
                    ),
                    None,
                    None,
                ));
            }
        }

        match self.fields() {
            Ok(fields) => {
                for field in fields {
                    if field.origin == FieldOrigin::UnknownSubrecord {
                        report.push(self.diagnostic(
                            DiagnosticSeverity::Warning,
                            DiagnosticCode::UnknownSubrecord,
                            format!(
                                "subrecord {} is not declared by the package",
                                field.subrecord_signature
                            ),
                            None,
                            Some(field.span),
                        ));
                    } else if field.origin == FieldOrigin::UnmatchedKnownSubrecord {
                        report.push(self.diagnostic(
                            DiagnosticSeverity::Error,
                            DiagnosticCode::InvalidOrder,
                            format!(
                                "subrecord {} appears outside its schema position",
                                field.subrecord_signature
                            ),
                            None,
                            Some(field.span),
                        ));
                    }
                    if matches!(
                        field.origin,
                        FieldOrigin::Schema | FieldOrigin::CustomDecoder
                    ) {
                        self.validate_callback_value(
                            &field.path,
                            &field.value,
                            field.span,
                            &mut report,
                        );
                    }
                }
            }
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
            }
        }
        report
    }

    fn validate_callback_value(
        &self,
        path: &str,
        value: &FieldValue<'_>,
        span: ByteSpan,
        report: &mut ValidationReport,
    ) {
        let node = self
            .context
            .registry()
            .get_node(self.record.header.signature, path);
        if let (
            Some(SchemaNode {
                kind:
                    SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::String { string },
                    },
                ..
            }),
            FieldValue::String(value),
        ) = (node, value)
        {
            if !value.is_empty()
                && !string.allowed_values.is_empty()
                && !string
                    .allowed_values
                    .iter()
                    .any(|allowed| allowed == value.as_ref())
            {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidStringEnumeration,
                    format!("<Unknown: {value}>"),
                    node,
                    Some(span),
                ));
            }
        }
        for binding in self
            .context
            .registry()
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == path && crate::handler::runs_during_validation(binding)
            })
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            let handler_value = value.to_handler_value();
            let outcome = self.context.handlers().invoke(
                binding,
                HandlerRecordContext::new(
                    self.record.header.signature,
                    self.record.header.form_id,
                    self.record.header.form_version,
                    self.context.registry().package().manifest().game,
                ),
                HandlerPhase::Validation,
                Some(&handler_value),
                None,
            );
            let message = match outcome {
                Ok(HandlerOutput::Text(message)) if !message.is_empty() => Some(message),
                Ok(HandlerOutput::None | HandlerOutput::Text(_)) => None,
                Ok(_) => Some("validation callback returned an invalid result".to_owned()),
                Err(error) => Some(error.to_string()),
            };
            if let Some(message) = message {
                let node = self
                    .context
                    .registry()
                    .get_node(self.record.header.signature, path);
                report.push(self.diagnostic(
                    callback_validation_severity(&message),
                    DiagnosticCode::CallbackValidation,
                    message,
                    node,
                    Some(span),
                ));
            }
        }
        self.validate_callback_descendants(value, report);
    }

    fn validate_callback_descendants(&self, value: &FieldValue<'_>, report: &mut ValidationReport) {
        match value {
            FieldValue::Struct(values) => {
                for value in values {
                    self.validate_callback_value(&value.path, &value.value, value.span, report);
                }
            }
            FieldValue::Array(values) => {
                for value in values {
                    self.validate_callback_descendants(value, report);
                }
            }
            _ => {}
        }
    }

    fn decode_node<'a>(
        &self,
        node: &SchemaNode,
        payload: &'a [u8],
        current: &'a [u8],
        frame: DecodeFrame<'_, 'a>,
        field_values: &mut BTreeMap<String, i64>,
    ) -> Result<(FieldValue<'a>, usize, Option<String>)> {
        if !self.node_applies(node, payload, field_values)? {
            return Ok((FieldValue::Absent, 0, None));
        }

        let (decoded, consumed, effective_path) = match &node.kind {
            SchemaNodeKind::Primitive { primitive } => {
                let consumed = node_data_size(node, current, self.localized)?;
                let data = current
                    .get(..consumed)
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "primitive needs {consumed} bytes, only {} remain",
                            current.len()
                        ),
                    })?;
                decode_primitive(primitive, data, self.localized, &node.path)
                    .map(|value| (value, consumed, None))
            }
            SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => {
                let optional_from = match &node.kind {
                    SchemaNodeKind::OptionalStruct { optional_from, .. } => {
                        Some(*optional_from as usize)
                    }
                    _ => None,
                };
                let mut values: Vec<NamedValue<'a>> = Vec::with_capacity(fields.len());
                let mut cursor: usize = 0;
                let mut optional_suffix_absent = false;
                for (index, field) in fields.iter().enumerate() {
                    if !self.node_applies(field, payload, field_values)? {
                        values.push(NamedValue {
                            node_id: field.id,
                            path: field.path.clone(),
                            effective_path: None,
                            name: field.name.clone(),
                            span: ByteSpan {
                                start: frame.offset + cursor,
                                end: frame.offset + cursor,
                            },
                            value: FieldValue::Absent,
                        });
                        continue;
                    }
                    let remaining: &'a [u8] =
                        current.get(cursor..).ok_or_else(|| SemanticError::Decode {
                            path: field.path.clone(),
                            message: "struct cursor exceeded payload".to_owned(),
                        })?;
                    let field_is_optional = optional_from.is_some_and(|start| index >= start);
                    let field_does_not_fit =
                        fixed_node_size(field).is_some_and(|size| size > remaining.len());
                    if field_is_optional
                        && (optional_suffix_absent || remaining.is_empty() || field_does_not_fit)
                    {
                        optional_suffix_absent = true;
                        values.push(NamedValue {
                            node_id: field.id,
                            path: field.path.clone(),
                            effective_path: None,
                            name: field.name.clone(),
                            span: ByteSpan {
                                start: frame.offset + cursor,
                                end: frame.offset + cursor,
                            },
                            value: FieldValue::Absent,
                        });
                        continue;
                    }
                    let child_frame = DecodeFrame {
                        offset: frame.offset + cursor,
                        source_subrecord_index: frame.source_subrecord_index,
                        sibling_values: &values,
                        array_indices: frame.array_indices,
                    };
                    let (value, consumed, effective_path): (FieldValue<'a>, usize, Option<String>) =
                        self.decode_node(field, payload, remaining, child_frame, field_values)?;
                    values.push(NamedValue {
                        node_id: field.id,
                        path: field.path.clone(),
                        effective_path,
                        name: field.name.clone(),
                        span: ByteSpan {
                            start: frame.offset + cursor,
                            end: frame.offset + cursor + consumed,
                        },
                        value,
                    });
                    cursor = cursor
                        .checked_add(consumed)
                        .ok_or_else(|| SemanticError::Decode {
                            path: field.path.clone(),
                            message: "struct cursor overflowed".to_owned(),
                        })?;
                }
                Ok((FieldValue::Struct(values), cursor, None))
            }
            SchemaNodeKind::Array { element, count } => {
                let (prefix_size, element_count): (usize, Option<usize>) = match count {
                    ArrayCount::Fixed { count } => (0, Some(*count as usize)),
                    ArrayCount::Prefixed {
                        integer,
                        terminator,
                    } => {
                        let count: u64 = decode_unsigned_integer(*integer, current, &node.path)?;
                        let count: usize =
                            usize::try_from(count).map_err(|_| SemanticError::Decode {
                                path: node.path.clone(),
                                message: "array count exceeds platform size".to_owned(),
                            })?;
                        (
                            array_prefix_size(
                                integer.width as usize,
                                *terminator,
                                current,
                                &node.path,
                            )?,
                            Some(count),
                        )
                    }
                    ArrayCount::PackedPrefixed { square, terminator } => {
                        let (count, width) = decode_packed_unsigned(current, &node.path)?;
                        let count = array_count_value(count, *square, &node.path)?;
                        (
                            array_prefix_size(width, *terminator, current, &node.path)?,
                            Some(count),
                        )
                    }
                    ArrayCount::SquaredPrefixed {
                        integer,
                        terminator,
                    } => {
                        let count = decode_unsigned_integer(*integer, current, &node.path)?;
                        let count = array_count_value(count, true, &node.path)?;
                        (
                            array_prefix_size(
                                integer.width as usize,
                                *terminator,
                                current,
                                &node.path,
                            )?,
                            Some(count),
                        )
                    }
                    ArrayCount::Remainder => (0, None),
                    ArrayCount::Expression { .. } | ArrayCount::Callback { .. } => (
                        0,
                        Some(self.resolve_array_count(
                            node,
                            count,
                            payload,
                            field_values,
                            frame,
                        )?),
                    ),
                };
                if prefix_size > current.len() {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "array prefix needs {prefix_size} bytes, payload has {}",
                            current.len()
                        ),
                    });
                }
                let mut values: Vec<FieldValue<'a>> =
                    Vec::with_capacity(element_count.unwrap_or_default());
                let mut cursor = prefix_size;
                while element_count.is_none_or(|count| values.len() < count) {
                    if cursor == current.len() && element_count.is_none() {
                        break;
                    }
                    let remaining = current.get(cursor..).ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "array cursor exceeded payload".to_owned(),
                    })?;
                    if !self.should_include_array_element(node, remaining, frame)? {
                        break;
                    }
                    let mut child_array_indices = frame.array_indices.to_vec();
                    child_array_indices.push(values.len());
                    let child_frame = DecodeFrame {
                        offset: frame.offset + cursor,
                        source_subrecord_index: frame.source_subrecord_index,
                        sibling_values: frame.sibling_values,
                        array_indices: &child_array_indices,
                    };
                    let (value, consumed, _) =
                        self.decode_node(element, payload, remaining, child_frame, field_values)?;
                    if consumed == 0 && element_count.is_none() {
                        return Err(SemanticError::Decode {
                            path: element.path.clone(),
                            message: "array element consumed no bytes".to_owned(),
                        });
                    }
                    let end =
                        cursor
                            .checked_add(consumed)
                            .ok_or_else(|| SemanticError::Decode {
                                path: node.path.clone(),
                                message: "array cursor overflowed".to_owned(),
                            })?;
                    if end > current.len() {
                        return Err(SemanticError::Decode {
                            path: element.path.clone(),
                            message: format!(
                                "array element needs {consumed} bytes, only {} remain",
                                remaining.len()
                            ),
                        });
                    }
                    values.push(value);
                    cursor = end;
                }
                Ok((FieldValue::Array(values), cursor, None))
            }
            SchemaNodeKind::Union { selector, variants } => {
                let index =
                    self.select_union_index(node, selector, payload, field_values, frame)?;
                let variant: &SchemaNode =
                    variants.get(index).ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!("union variant {index} does not exist"),
                    })?;
                let (value, consumed, selected) =
                    self.decode_node(variant, payload, current, frame, field_values)?;
                Ok((
                    value,
                    consumed,
                    Some(selected.unwrap_or_else(|| variant.path.clone())),
                ))
            }
            SchemaNodeKind::Custom { decoder, .. } => {
                let decoded = self
                    .context
                    .decoders()
                    .get(decoder)
                    .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                    .decode(current)?;
                if decoded.consumed > current.len() {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "custom decoder consumed {} bytes, only {} remain",
                            decoded.consumed,
                            current.len()
                        ),
                    });
                }
                Ok((decoded.value, decoded.consumed, None))
            }
            SchemaNodeKind::Terminated { terminator, child } => {
                let (value, body_size, effective_path) =
                    self.decode_node(child, payload, current, frame, field_values)?;
                let actual = current
                    .get(body_size)
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "terminated value is missing its terminator".to_owned(),
                    })?;
                if actual != terminator {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "expected terminator 0x{terminator:02X}, got 0x{actual:02X}"
                        ),
                    });
                }
                let consumed = body_size
                    .checked_add(1)
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "terminated value size overflowed".to_owned(),
                    })?;
                Ok((value, consumed, effective_path))
            }
            SchemaNodeKind::Compressed { .. } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "compressed nodes require a registered custom decoder".to_owned(),
            }),
            SchemaNodeKind::Reference { target } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: format!("unresolved schema reference {target}"),
            }),
            SchemaNodeKind::Sequence { .. }
            | SchemaNodeKind::Unordered { .. }
            | SchemaNodeKind::Choice { .. }
            | SchemaNodeKind::SelectedChoice { .. }
            | SchemaNodeKind::Repeat { .. }
            | SchemaNodeKind::Subrecord { .. } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "container node cannot decode a payload directly".to_owned(),
            }),
        }?;
        let normalized = self
            .context
            .apply_normalizers(&node.path, self.record, decoded)?;
        let value = apply_float_read_semantics(node, normalized)?;
        match &value {
            FieldValue::Int(value) => {
                field_values.insert(node.path.clone(), *value);
            }
            FieldValue::UInt(value) | FieldValue::Flags { value, .. } => {
                if let Ok(value) = i64::try_from(*value) {
                    field_values.insert(node.path.clone(), value);
                }
            }
            FieldValue::Enumeration { value, .. } => {
                field_values.insert(node.path.clone(), *value);
            }
            FieldValue::FormId { value, .. } => {
                field_values.insert(node.path.clone(), i64::from(value.0));
            }
            _ => {}
        }
        Ok((value, consumed, effective_path))
    }

    fn select_union_index(
        &self,
        node: &SchemaNode,
        selector: &UnionSelector,
        payload: &[u8],
        field_values: &BTreeMap<String, i64>,
        frame: DecodeFrame<'_, '_>,
    ) -> Result<usize> {
        let selected = match selector {
            UnionSelector::Expression(expression) => {
                let context = EvalContext {
                    payload,
                    field_values,
                    form_version: self.record.header.form_version,
                    record_signature: self.record.header.signature.into(),
                };
                match expression.evaluate(&context, 1024)? {
                    EvalValue::Int(value) => value,
                    _ => {
                        return Err(SemanticError::Decode {
                            path: node.path.clone(),
                            message: "union selector did not return an integer".to_owned(),
                        });
                    }
                }
            }
            UnionSelector::Callback { callback_id } => {
                let value_scope =
                    FieldValue::Struct(frame.sibling_values.to_vec()).to_handler_value();
                self.invoke_integer_callback(
                    node,
                    callback_id,
                    payload,
                    frame,
                    HandlerPhase::UnionSelection,
                    Some(&value_scope),
                )?
            }
        };
        if selected < 0 {
            return Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "union selector returned a negative index".to_owned(),
            });
        }
        usize::try_from(selected).map_err(|_| SemanticError::Decode {
            path: node.path.clone(),
            message: "union selector exceeds platform size".to_owned(),
        })
    }

    fn resolve_array_count(
        &self,
        node: &SchemaNode,
        count: &ArrayCount,
        payload: &[u8],
        field_values: &BTreeMap<String, i64>,
        frame: DecodeFrame<'_, '_>,
    ) -> Result<usize> {
        let value = match count {
            ArrayCount::Expression { expression } => {
                let context = EvalContext {
                    payload,
                    field_values,
                    form_version: self.record.header.form_version,
                    record_signature: self.record.header.signature.into(),
                };
                match expression.evaluate(&context, 1024)? {
                    EvalValue::Int(value) => value,
                    _ => {
                        return Err(SemanticError::Decode {
                            path: node.path.clone(),
                            message: "array count expression did not return an integer".to_owned(),
                        });
                    }
                }
            }
            ArrayCount::Callback { callback_id } => self.invoke_integer_callback(
                node,
                callback_id,
                payload,
                frame,
                HandlerPhase::ArrayCount,
                None,
            )?,
            _ => {
                return Err(SemanticError::Decode {
                    path: node.path.clone(),
                    message: "array count is not dynamically selected".to_owned(),
                });
            }
        };
        if value < 0 {
            return Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "array count returned a negative value".to_owned(),
            });
        }
        usize::try_from(value).map_err(|_| SemanticError::Decode {
            path: node.path.clone(),
            message: "array count exceeds platform size".to_owned(),
        })
    }

    fn should_include_array_element(
        &self,
        node: &SchemaNode,
        remaining: &[u8],
        frame: DecodeFrame<'_, '_>,
    ) -> Result<bool> {
        let Some(binding) = self
            .context
            .registry()
            .package()
            .callback_bindings()
            .iter()
            .find(|binding| {
                binding.path == node.path && binding.callback_id == "array.should_include"
            })
        else {
            return Ok(true);
        };
        let value = FieldValue::Bytes(Cow::Owned(remaining.to_vec()));
        match self.context.handlers().invoke_with_records(
            binding,
            HandlerRecordContext::new(
                self.record.header.signature,
                self.record.header.form_id,
                self.record.header.form_version,
                self.context.registry().package().manifest().game,
            ),
            HandlerInvocationAccess::read_only_subrecord_with_scope(
                self.record,
                frame.source_subrecord_index,
                None,
            )
            .with_array_indices(frame.array_indices),
            HandlerPhase::ArrayElementInclusion,
            Some(&value),
            None,
        )? {
            HandlerOutput::Integer(value) => Ok(value != 0),
            _ => Err(SemanticError::Handler {
                handler: binding.callback_id.clone(),
                message: "array inclusion callback returned a non-integer result".to_owned(),
            }),
        }
    }

    fn invoke_integer_callback(
        &self,
        node: &SchemaNode,
        callback_id: &str,
        payload: &[u8],
        frame: DecodeFrame<'_, '_>,
        phase: HandlerPhase,
        value_scope: Option<&FieldValue<'static>>,
    ) -> Result<i64> {
        let binding = self
            .context
            .registry()
            .package()
            .callback_bindings()
            .iter()
            .find(|binding| {
                binding.path == node.path && binding.callback_id.as_str() == callback_id
            })
            .ok_or_else(|| SemanticError::Handler {
                handler: callback_id.to_owned(),
                message: format!("schema node {} has no callback binding", node.path),
            })?;
        let value = FieldValue::Bytes(Cow::Owned(payload.to_vec()));
        match self.context.handlers().invoke_with_records(
            binding,
            HandlerRecordContext::new(
                self.record.header.signature,
                self.record.header.form_id,
                self.record.header.form_version,
                self.context.registry().package().manifest().game,
            ),
            HandlerInvocationAccess::read_only_subrecord_with_scope(
                self.record,
                frame.source_subrecord_index,
                value_scope,
            )
            .with_array_indices(frame.array_indices),
            phase,
            Some(&value),
            None,
        )? {
            HandlerOutput::Integer(value) => Ok(value),
            _ => Err(SemanticError::Handler {
                handler: callback_id.to_owned(),
                message: "callback returned a non-integer result".to_owned(),
            }),
        }
    }

    fn node_applies(
        &self,
        node: &SchemaNode,
        payload: &[u8],
        field_values: &BTreeMap<String, i64>,
    ) -> Result<bool> {
        let Some(condition) = &node.condition else {
            return Ok(true);
        };
        let context = EvalContext {
            payload,
            field_values,
            form_version: self.record.header.form_version,
            record_signature: self.record.header.signature.into(),
        };
        match condition.evaluate(&context, 1024)? {
            EvalValue::Bool(value) => Ok(value),
            _ => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "field condition did not return a boolean".to_owned(),
            }),
        }
    }

    fn diagnostic(
        &self,
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        message: String,
        node: Option<&SchemaNode>,
        span: Option<ByteSpan>,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message,
            record_signature: self.record.header.signature,
            form_id: self.record.header.form_id,
            node_id: node.map(|value| value.id),
            path: node.map(|value| value.path.clone()),
            span,
        }
    }
}

fn array_element_field<'value, 'record>(
    scope: &'value FieldValue<'record>,
    array_path: &str,
    index: usize,
    field_path: &str,
) -> Result<&'value NamedValue<'record>> {
    let array = find_named_value(scope, array_path).ok_or_else(|| SemanticError::Decode {
        path: array_path.to_owned(),
        message: "array does not exist in the decoded record".to_owned(),
    })?;
    let FieldValue::Array(elements) = &array.value else {
        return Err(SemanticError::Decode {
            path: array_path.to_owned(),
            message: "selected path is not an array".to_owned(),
        });
    };
    let element = elements.get(index).ok_or_else(|| SemanticError::Decode {
        path: array_path.to_owned(),
        message: format!("array element {index} does not exist"),
    })?;
    find_named_value(element, field_path).ok_or_else(|| SemanticError::Decode {
        path: field_path.to_owned(),
        message: "field does not exist in the selected array element".to_owned(),
    })
}

fn missing_required_subrecords<'a>(
    root: &'a SchemaNode,
    assignments: &[&SchemaNode],
) -> Vec<&'a SchemaNode> {
    fn is_assigned(node: &SchemaNode, assignments: &[&SchemaNode]) -> bool {
        assignments
            .iter()
            .any(|assignment| assignment.id == node.id)
    }

    fn subtree_is_assigned(node: &SchemaNode, assignments: &[&SchemaNode]) -> bool {
        if is_assigned(node, assignments) {
            return true;
        }
        match &node.kind {
            SchemaNodeKind::Sequence { children } | SchemaNodeKind::Unordered { children } => {
                children
                    .iter()
                    .any(|child| subtree_is_assigned(child, assignments))
            }
            SchemaNodeKind::Choice { alternatives }
            | SchemaNodeKind::SelectedChoice { alternatives, .. } => alternatives
                .iter()
                .any(|alternative| subtree_is_assigned(alternative, assignments)),
            SchemaNodeKind::Repeat { child, .. } => subtree_is_assigned(child, assignments),
            _ => false,
        }
    }

    fn collect<'a>(
        node: &'a SchemaNode,
        assignments: &[&SchemaNode],
        active: bool,
        root: bool,
        output: &mut Vec<&'a SchemaNode>,
    ) {
        match &node.kind {
            SchemaNodeKind::Subrecord { .. } => {
                if active && node.required && !is_assigned(node, assignments) {
                    output.push(node);
                }
            }
            SchemaNodeKind::Sequence { children } | SchemaNodeKind::Unordered { children } => {
                let children_active =
                    active && (root || node.required || subtree_is_assigned(node, assignments));
                for child in children {
                    collect(child, assignments, children_active, false, output);
                }
            }
            SchemaNodeKind::Choice { alternatives }
            | SchemaNodeKind::SelectedChoice { alternatives, .. } => {
                for alternative in alternatives
                    .iter()
                    .filter(|alternative| subtree_is_assigned(alternative, assignments))
                {
                    collect(alternative, assignments, active, false, output);
                }
            }
            SchemaNodeKind::Repeat { child, .. } => {
                let child_active = active && subtree_is_assigned(child, assignments);
                collect(child, assignments, child_active, false, output);
            }
            _ => {}
        }
    }

    let mut missing = Vec::new();
    collect(root, assignments, true, true, &mut missing);
    missing
}

fn callback_validation_severity(message: &str) -> DiagnosticSeverity {
    if message.starts_with("<Warning:") {
        DiagnosticSeverity::Warning
    } else {
        DiagnosticSeverity::Error
    }
}

fn named_from_field<'a>(field: &Field<'a>) -> NamedValue<'a> {
    NamedValue {
        node_id: field.node_id,
        path: field.path.clone(),
        effective_path: None,
        name: field.name.clone(),
        span: field.span,
        value: field.value.clone(),
    }
}

fn find_named_value<'a, 'value>(
    value: &'value FieldValue<'a>,
    target_path: &str,
) -> Option<&'value NamedValue<'a>> {
    match value {
        FieldValue::Struct(values) => values.iter().find_map(|value| {
            (value.path == target_path
                || target_path
                    .strip_suffix("/payload")
                    .is_some_and(|parent| parent == value.path))
            .then_some(value)
            .or_else(|| find_named_value(&value.value, target_path))
        }),
        FieldValue::Array(values) => values
            .iter()
            .find_map(|value| find_named_value(value, target_path)),
        _ => None,
    }
}

fn repeat_position_value(index: usize, count: usize) -> NamedValue<'static> {
    let scalar = |name: &str, value: usize| NamedValue {
        node_id: bethkit_schema::SchemaNodeId(u32::MAX),
        path: String::new(),
        effective_path: None,
        name: name.to_owned(),
        span: ByteSpan { start: 0, end: 0 },
        value: FieldValue::UInt(value as u64),
    };
    NamedValue {
        node_id: bethkit_schema::SchemaNodeId(u32::MAX),
        path: String::new(),
        effective_path: None,
        name: "Bethkit Repeat Position".to_owned(),
        span: ByteSpan { start: 0, end: 0 },
        value: FieldValue::Struct(vec![scalar("Index", index), scalar("Count", count)]),
    }
}

fn decode_primitive<'a>(
    primitive: &PrimitiveType,
    data: &'a [u8],
    localized: bool,
    path: &str,
) -> Result<FieldValue<'a>> {
    match primitive {
        PrimitiveType::Integer { integer } => decode_integer(*integer, data, path),
        PrimitiveType::PackedUnsigned => {
            let (value, width) = decode_packed_unsigned(data, path)?;
            if width != data.len() {
                return Err(decode_length_error(path, width, data.len()));
            }
            Ok(FieldValue::UInt(value))
        }
        PrimitiveType::Float {
            width, byte_order, ..
        } => decode_float(*width, *byte_order, data, path),
        PrimitiveType::String { string } => decode_string(string, data, localized, path),
        PrimitiveType::Bytes { length } => {
            if let Some(expected) = length {
                if data.len() != *expected as usize {
                    return Err(decode_length_error(path, *expected as usize, data.len()));
                }
            }
            Ok(FieldValue::Bytes(Cow::Borrowed(data)))
        }
        PrimitiveType::FormId { targets } => {
            if data.len() != 4 {
                return Err(decode_length_error(path, 4, data.len()));
            }
            let raw: u32 = u32::from_le_bytes(data.try_into().expect("FormID length was checked"));
            Ok(FieldValue::FormId {
                value: FormId(raw),
                targets: targets.iter().copied().map(Signature::from).collect(),
            })
        }
        PrimitiveType::Enumeration { integer, values } => {
            let raw: i64 = integer_as_i64(*integer, data, path)?;
            let name: Option<String> = values
                .iter()
                .find(|(value, _)| *value == raw)
                .map(|(_, name)| name.clone());
            Ok(FieldValue::Enumeration { value: raw, name })
        }
        PrimitiveType::Flags { integer, bits } => {
            let raw: u64 = integer_as_u64(*integer, data, path)?;
            let active: Vec<String> = bits
                .iter()
                .filter(|(bit, _)| *bit < 64 && raw & (1_u64 << bit) != 0)
                .map(|(_, name)| name.clone())
                .collect();
            Ok(FieldValue::Flags { value: raw, active })
        }
        PrimitiveType::Unused { length } => {
            if data.len() != *length as usize {
                return Err(decode_length_error(path, *length as usize, data.len()));
            }
            Ok(FieldValue::Bytes(Cow::Borrowed(data)))
        }
    }
}

fn apply_float_read_semantics<'a>(
    node: &SchemaNode,
    value: FieldValue<'a>,
) -> Result<FieldValue<'a>> {
    let SchemaNodeKind::Primitive {
        primitive: PrimitiveType::Float { scale, digits, .. },
    } = &node.kind
    else {
        return Ok(value);
    };
    let FieldValue::Float(value) = value else {
        return Err(SemanticError::Decode {
            path: node.path.clone(),
            message: "float schema node decoded a non-floating-point value".to_owned(),
        });
    };
    Ok(FieldValue::Float(float_from_raw(value, *scale, *digits)))
}

fn decode_integer<'a>(integer: IntegerType, data: &'a [u8], path: &str) -> Result<FieldValue<'a>> {
    if integer.signed {
        integer_as_i64(integer, data, path).map(FieldValue::Int)
    } else {
        integer_as_u64(integer, data, path).map(FieldValue::UInt)
    }
}

fn decode_unsigned_integer(integer: IntegerType, data: &[u8], path: &str) -> Result<u64> {
    if integer.signed {
        return Err(SemanticError::Decode {
            path: path.to_owned(),
            message: "array count prefix must be unsigned".to_owned(),
        });
    }
    let width: usize = integer.width as usize;
    let prefix: &[u8] = data
        .get(..width)
        .ok_or_else(|| decode_length_error(path, width, data.len()))?;
    integer_as_u64(integer, prefix, path)
}

fn decode_packed_unsigned(data: &[u8], path: &str) -> Result<(u64, usize)> {
    let first = *data
        .first()
        .ok_or_else(|| decode_length_error(path, 1, 0))?;
    let width = match first & 0x03 {
        0 | 3 => 1,
        1 => 2,
        2 => 4,
        _ => unreachable!("two-bit packed width selector"),
    };
    let bytes = data
        .get(..width)
        .ok_or_else(|| decode_length_error(path, width, data.len()))?;
    let raw = match width {
        1 => u64::from(bytes[0]),
        2 => u64::from(u16::from_le_bytes(
            bytes.try_into().expect("packed u16 length was checked"),
        )),
        4 => u64::from(u32::from_le_bytes(
            bytes.try_into().expect("packed u32 length was checked"),
        )),
        _ => unreachable!("packed integer width was validated"),
    };
    Ok((raw >> 2, width))
}

fn array_count_value(value: u64, square: bool, path: &str) -> Result<usize> {
    let value = if square {
        value
            .checked_mul(value)
            .ok_or_else(|| SemanticError::Decode {
                path: path.to_owned(),
                message: "matrix element count overflowed".to_owned(),
            })?
    } else {
        value
    };
    usize::try_from(value).map_err(|_| SemanticError::Decode {
        path: path.to_owned(),
        message: "array count exceeds platform size".to_owned(),
    })
}

fn integer_as_u64(integer: IntegerType, data: &[u8], path: &str) -> Result<u64> {
    if data.len() != integer.width as usize {
        return Err(decode_length_error(
            path,
            integer.width as usize,
            data.len(),
        ));
    }
    let value: u64 = match (integer.width, integer.byte_order) {
        (1, _) => u64::from(data[0]),
        (2, ByteOrder::LittleEndian) => u64::from(u16::from_le_bytes([data[0], data[1]])),
        (2, ByteOrder::BigEndian) => u64::from(u16::from_be_bytes([data[0], data[1]])),
        (4, ByteOrder::LittleEndian) => u64::from(u32::from_le_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (4, ByteOrder::BigEndian) => u64::from(u32::from_be_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            u64::from_le_bytes(data.try_into().expect("integer length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            u64::from_be_bytes(data.try_into().expect("integer length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported integer width {}", integer.width),
            });
        }
    };
    Ok(value)
}

fn integer_as_i64(integer: IntegerType, data: &[u8], path: &str) -> Result<i64> {
    if !integer.signed {
        return i64::try_from(integer_as_u64(integer, data, path)?).map_err(|_| {
            SemanticError::Decode {
                path: path.to_owned(),
                message: "unsigned integer exceeds i64".to_owned(),
            }
        });
    }
    if data.len() != integer.width as usize {
        return Err(decode_length_error(
            path,
            integer.width as usize,
            data.len(),
        ));
    }
    let value: i64 = match (integer.width, integer.byte_order) {
        (1, _) => i64::from(data[0] as i8),
        (2, ByteOrder::LittleEndian) => i64::from(i16::from_le_bytes([data[0], data[1]])),
        (2, ByteOrder::BigEndian) => i64::from(i16::from_be_bytes([data[0], data[1]])),
        (4, ByteOrder::LittleEndian) => i64::from(i32::from_le_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (4, ByteOrder::BigEndian) => i64::from(i32::from_be_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            i64::from_le_bytes(data.try_into().expect("integer length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            i64::from_be_bytes(data.try_into().expect("integer length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported integer width {}", integer.width),
            });
        }
    };
    Ok(value)
}

fn decode_float<'a>(
    width: u8,
    byte_order: ByteOrder,
    data: &'a [u8],
    path: &str,
) -> Result<FieldValue<'a>> {
    if data.len() != width as usize {
        return Err(decode_length_error(path, width as usize, data.len()));
    }
    let value: f64 = match (width, byte_order) {
        (4, ByteOrder::LittleEndian) => f64::from(f32::from_le_bytes(
            data.try_into().expect("float length was checked"),
        )),
        (4, ByteOrder::BigEndian) => f64::from(f32::from_be_bytes(
            data.try_into().expect("float length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            f64::from_le_bytes(data.try_into().expect("float length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            f64::from_be_bytes(data.try_into().expect("float length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported float width {width}"),
            });
        }
    };
    Ok(FieldValue::Float(value))
}

fn decode_string<'a>(
    string: &StringType,
    data: &'a [u8],
    localized: bool,
    path: &str,
) -> Result<FieldValue<'a>> {
    if is_localized_string(string) && localized {
        if data.len() != 4 {
            return Err(decode_length_error(path, 4, data.len()));
        }
        let id: u32 = u32::from_le_bytes(
            data.try_into()
                .expect("localized string length was checked"),
        );
        return Ok(FieldValue::UInt(u64::from(id)));
    }
    let bytes: &'a [u8] = string_body(string, data, path)?;
    let bytes: &'a [u8] = if string.zero_terminated {
        let end: usize = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        &bytes[..end]
    } else {
        bytes
    };
    let value: Cow<'a, str> = match text_encoding(string) {
        "utf8" => {
            Cow::Borrowed(
                std::str::from_utf8(bytes).map_err(|error| SemanticError::Decode {
                    path: path.to_owned(),
                    message: error.to_string(),
                })?,
            )
        }
        "windows_1252" => {
            let (value, had_errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
            if had_errors {
                return Err(SemanticError::Decode {
                    path: path.to_owned(),
                    message: "invalid Windows-1252 byte sequence".to_owned(),
                });
            }
            value
        }
        encoding => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("string encoding {encoding} requires a custom decoder"),
            });
        }
    };
    Ok(FieldValue::String(value))
}

fn is_localized_string(string: &StringType) -> bool {
    string.localized || string.encoding == "localized"
}

fn text_encoding(string: &StringType) -> &str {
    if string.encoding == "localized" {
        "windows_1252"
    } else {
        &string.encoding
    }
}

fn string_body<'a>(string: &StringType, data: &'a [u8], path: &str) -> Result<&'a [u8]> {
    let representation: &'a [u8] = if let Some(terminator) = string.trailing_terminator {
        let (&actual, body) = data.split_last().ok_or_else(|| SemanticError::Decode {
            path: path.to_owned(),
            message: "string is missing its structural terminator".to_owned(),
        })?;
        if actual != terminator {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!(
                    "expected structural terminator 0x{terminator:02X}, got 0x{actual:02X}"
                ),
            });
        }
        body
    } else {
        data
    };
    if let Some(prefix) = string.length_prefix {
        if representation.len() < prefix.offset as usize {
            return Err(decode_length_error(
                path,
                prefix.offset as usize,
                representation.len(),
            ));
        }
        let length: usize = read_string_length(prefix.width, representation, path)?;
        let end: usize = (prefix.offset as usize)
            .checked_add(length)
            .ok_or_else(|| SemanticError::Decode {
                path: path.to_owned(),
                message: "string length overflowed".to_owned(),
            })?;
        if end != representation.len() {
            return Err(decode_length_error(path, end, representation.len()));
        }
        return Ok(&representation[prefix.offset as usize..end]);
    }
    if let Some(length) = string.fixed_length {
        if representation.len() != length as usize {
            return Err(decode_length_error(
                path,
                length as usize,
                representation.len(),
            ));
        }
    }
    Ok(representation)
}

fn read_string_length(width: u8, data: &[u8], path: &str) -> Result<usize> {
    let length: u32 = match width {
        1 if !data.is_empty() => u32::from(data[0]),
        2 if data.len() >= 2 => u32::from(u16::from_le_bytes([data[0], data[1]])),
        4 if data.len() >= 4 => {
            u32::from_le_bytes(data[..4].try_into().expect("prefix length was checked"))
        }
        1 | 2 | 4 => return Err(decode_length_error(path, width as usize, data.len())),
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported string length prefix width {width}"),
            });
        }
    };
    usize::try_from(length).map_err(|_| SemanticError::Decode {
        path: path.to_owned(),
        message: "string length exceeds platform size".to_owned(),
    })
}

fn node_data_size(node: &SchemaNode, data: &[u8], localized: bool) -> Result<usize> {
    node_data_size_with_resolver(node, data, localized, &mut |node, _| {
        Err(SemanticError::Decode {
            path: node.path.clone(),
            message: "dynamic array count requires a semantic context".to_owned(),
        })
    })
}

fn node_data_size_with_resolver<F>(
    node: &SchemaNode,
    data: &[u8],
    localized: bool,
    resolve_count: &mut F,
) -> Result<usize>
where
    F: FnMut(&SchemaNode, &ArrayCount) -> Result<usize>,
{
    match &node.kind {
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::String { string },
        } => {
            if is_localized_string(string) && localized {
                return Ok(4);
            }
            let trailing: usize = usize::from(string.trailing_terminator.is_some());
            if let Some(prefix) = string.length_prefix {
                let length: usize = read_string_length(prefix.width, data, &node.path)?;
                return (prefix.offset as usize)
                    .checked_add(length)
                    .and_then(|value| value.checked_add(trailing))
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "string size overflowed".to_owned(),
                    });
            }
            if let Some(length) = string.fixed_length {
                return (length as usize).checked_add(trailing).ok_or_else(|| {
                    SemanticError::Decode {
                        path: node.path.clone(),
                        message: "string size overflowed".to_owned(),
                    }
                });
            }
            if string.zero_terminated {
                let body_length: usize = data
                    .iter()
                    .position(|byte| *byte == 0)
                    .map_or(data.len(), |index| index + 1);
                return body_length
                    .checked_add(trailing)
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "string size overflowed".to_owned(),
                    });
            }
            Ok(data.len())
        }
        SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => {
            let mut cursor = 0_usize;
            let optional_from = match &node.kind {
                SchemaNodeKind::OptionalStruct { optional_from, .. } => {
                    Some(*optional_from as usize)
                }
                _ => None,
            };
            for (index, field) in fields.iter().enumerate() {
                let remaining = data.get(cursor..).ok_or_else(|| SemanticError::Decode {
                    path: field.path.clone(),
                    message: "struct size cursor exceeded payload".to_owned(),
                })?;
                if optional_from.is_some_and(|start| index >= start)
                    && (remaining.is_empty()
                        || fixed_node_size(field).is_some_and(|size| size > remaining.len()))
                {
                    break;
                }
                let consumed =
                    node_data_size_with_resolver(field, remaining, localized, resolve_count)?;
                cursor = cursor
                    .checked_add(consumed)
                    .ok_or_else(|| SemanticError::Decode {
                        path: field.path.clone(),
                        message: "struct size overflowed".to_owned(),
                    })?;
                if cursor > data.len() {
                    return Err(SemanticError::Decode {
                        path: field.path.clone(),
                        message: format!(
                            "field needs {consumed} bytes, only {} remain",
                            remaining.len()
                        ),
                    });
                }
            }
            Ok(cursor)
        }
        SchemaNodeKind::Array { element, count } => {
            let (mut cursor, count) = match count {
                ArrayCount::Fixed { count } => (0, Some(*count as usize)),
                ArrayCount::Prefixed {
                    integer,
                    terminator,
                } => {
                    let count =
                        usize::try_from(decode_unsigned_integer(*integer, data, &node.path)?)
                            .map_err(|_| SemanticError::Decode {
                                path: node.path.clone(),
                                message: "array count exceeds platform size".to_owned(),
                            })?;
                    (
                        array_prefix_size(integer.width as usize, *terminator, data, &node.path)?,
                        Some(count),
                    )
                }
                ArrayCount::PackedPrefixed { square, terminator } => {
                    let (count, width) = decode_packed_unsigned(data, &node.path)?;
                    (
                        array_prefix_size(width, *terminator, data, &node.path)?,
                        Some(array_count_value(count, *square, &node.path)?),
                    )
                }
                ArrayCount::SquaredPrefixed {
                    integer,
                    terminator,
                } => {
                    let count = decode_unsigned_integer(*integer, data, &node.path)?;
                    (
                        array_prefix_size(integer.width as usize, *terminator, data, &node.path)?,
                        Some(array_count_value(count, true, &node.path)?),
                    )
                }
                ArrayCount::Remainder => (0, None),
                ArrayCount::Expression { .. } | ArrayCount::Callback { .. } => {
                    (0, Some(resolve_count(node, count)?))
                }
            };
            let mut decoded = 0_usize;
            while count.is_none_or(|count| decoded < count) {
                if cursor == data.len() && count.is_none() {
                    break;
                }
                let remaining = data.get(cursor..).ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: "array size cursor exceeded payload".to_owned(),
                })?;
                let consumed =
                    node_data_size_with_resolver(element, remaining, localized, resolve_count)?;
                if consumed == 0 {
                    return Err(SemanticError::Decode {
                        path: element.path.clone(),
                        message: "array element consumed no bytes".to_owned(),
                    });
                }
                cursor = cursor
                    .checked_add(consumed)
                    .ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "array size overflowed".to_owned(),
                    })?;
                if cursor > data.len() {
                    return Err(SemanticError::Decode {
                        path: element.path.clone(),
                        message: format!(
                            "array element needs {consumed} bytes, only {} remain",
                            remaining.len()
                        ),
                    });
                }
                decoded += 1;
            }
            Ok(cursor)
        }
        SchemaNodeKind::Terminated { terminator, child } => {
            let child_size = node_data_size_with_resolver(child, data, localized, resolve_count)?;
            let actual = data.get(child_size).ok_or_else(|| SemanticError::Decode {
                path: node.path.clone(),
                message: "terminated value is missing its terminator".to_owned(),
            })?;
            if actual != terminator {
                return Err(SemanticError::Decode {
                    path: node.path.clone(),
                    message: format!("expected terminator 0x{terminator:02X}, got 0x{actual:02X}"),
                });
            }
            child_size
                .checked_add(1)
                .ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: "terminated value size overflowed".to_owned(),
                })
        }
        _ => Ok(fixed_node_size(node).unwrap_or(data.len())),
    }
}

fn fixed_node_size(node: &SchemaNode) -> Option<usize> {
    match &node.kind {
        SchemaNodeKind::Primitive { primitive } => match primitive {
            PrimitiveType::Integer { integer } => Some(integer.width as usize),
            PrimitiveType::PackedUnsigned => None,
            PrimitiveType::Float { width, .. } => Some(*width as usize),
            PrimitiveType::String { string } if !is_localized_string(string) => {
                string.fixed_length.and_then(|value| {
                    (value as usize).checked_add(usize::from(string.trailing_terminator.is_some()))
                })
            }
            PrimitiveType::String { .. } => None,
            PrimitiveType::Bytes { length } => length.map(|value| value as usize),
            PrimitiveType::FormId { .. } => Some(4),
            PrimitiveType::Enumeration { integer, .. } | PrimitiveType::Flags { integer, .. } => {
                Some(integer.width as usize)
            }
            PrimitiveType::Unused { length } => Some(*length as usize),
        },
        SchemaNodeKind::Struct { fields } => fields.iter().try_fold(0_usize, |total, field| {
            total.checked_add(fixed_node_size(field)?)
        }),
        SchemaNodeKind::OptionalStruct { .. } => None,
        SchemaNodeKind::Array { element, count } => {
            let size: usize = fixed_node_size(element)?;
            match count {
                ArrayCount::Fixed { count } => size.checked_mul(*count as usize),
                ArrayCount::Prefixed { .. }
                | ArrayCount::PackedPrefixed { .. }
                | ArrayCount::SquaredPrefixed { .. }
                | ArrayCount::Expression { .. }
                | ArrayCount::Callback { .. }
                | ArrayCount::Remainder => None,
            }
        }
        SchemaNodeKind::Terminated { child, .. } => fixed_node_size(child)?.checked_add(1),
        _ => None,
    }
}

fn array_prefix_size(
    integer_size: usize,
    terminator: Option<u8>,
    data: &[u8],
    path: &str,
) -> Result<usize> {
    let Some(expected) = terminator else {
        return Ok(integer_size);
    };
    let actual = data
        .get(integer_size)
        .ok_or_else(|| SemanticError::Decode {
            path: path.to_owned(),
            message: "array count prefix is missing its terminator".to_owned(),
        })?;
    if *actual != expected {
        return Err(SemanticError::Decode {
            path: path.to_owned(),
            message: format!(
                "expected array prefix terminator 0x{expected:02X}, got 0x{actual:02X}"
            ),
        });
    }
    integer_size
        .checked_add(1)
        .ok_or_else(|| SemanticError::Decode {
            path: path.to_owned(),
            message: "array prefix size overflowed".to_owned(),
        })
}

fn decode_length_error(path: &str, expected: usize, actual: usize) -> SemanticError {
    SemanticError::Decode {
        path: path.to_owned(),
        message: format!("expected {expected} bytes, got {actual}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bethkit_core::{GameContext, Record};
    use bethkit_io::SliceCursor;
    use bethkit_schema::{
        BuiltInOperation, CallbackBinding, CallbackImplementation, Expression, HandlerRequirement,
        SchemaManifest, SchemaPackage, SchemaSignature, StringLengthPrefix, StringType,
        ValidationStatus, PACKAGE_FORMAT_VERSION,
    };

    use super::*;
    use crate::SemanticHandlerRegistry;

    /// Preserves xEdit validation warnings as non-fatal diagnostics.
    #[test]
    fn callback_validation_preserves_warning_severity(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // then
        assert_eq!(
            callback_validation_severity("<Warning: Could not resolve alias>"),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            callback_validation_severity("invalid value"),
            DiagnosticSeverity::Error
        );
        Ok(())
    }

    /// Ignores required descendants while their optional container is absent.
    #[test]
    fn required_validation_ignores_absent_optional_container(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let required_top = test_subrecord_node(1, "TEST/0:Top", *b"TOP_", true);
        let required_nested = test_subrecord_node(3, "TEST/1:Optional/0:End", *b"END_", true);
        let optional_container = SchemaNode {
            id: bethkit_schema::SchemaNodeId(2),
            path: "TEST/1:Optional".to_owned(),
            name: "Optional".to_owned(),
            required: false,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![required_nested],
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![required_top, optional_container],
            },
        };

        // when
        let missing = missing_required_subrecords(&root, &[]);

        // then
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].path, "TEST/0:Top");
        Ok(())
    }

    /// Enforces required descendants after an optional container becomes active.
    #[test]
    fn required_validation_checks_active_optional_container(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let marker = test_subrecord_node(2, "TEST/0:Optional/0:Marker", *b"MARK", false);
        let required_end = test_subrecord_node(3, "TEST/0:Optional/1:End", *b"END_", true);
        let optional_container = SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/0:Optional".to_owned(),
            name: "Optional".to_owned(),
            required: false,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![marker.clone(), required_end],
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![optional_container],
            },
        };

        // when
        let missing = missing_required_subrecords(&root, &[&marker]);

        // then
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].path, "TEST/0:Optional/1:End");
        Ok(())
    }

    /// Downgrades missing required fields only in xEdit-compatible validation.
    #[test]
    fn xedit_compatible_validation_warns_about_missing_required_fields(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![test_subrecord_node(1, "TEST/0:Top", *b"TOP_", true)],
            },
        };
        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let record_bytes = test_record_with_subrecords(b"TEST", &[]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;
        let view = context.view(&record, false)?;

        // when
        let strict = view.validate();
        let compatible = view.validate_with_mode(ValidationMode::XEditCompatible);

        // then
        assert!(strict.has_errors());
        assert!(!compatible.has_errors());
        assert!(compatible.diagnostics().iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::MissingRequired
                && diagnostic.severity == DiagnosticSeverity::Warning
        }));
        Ok(())
    }

    /// Accepts the reduced payload used by deleted records.
    #[test]
    fn required_validation_ignores_deleted_records(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![test_subrecord_node(1, "TEST/0:Top", *b"TOP_", true)],
            },
        };
        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut record_bytes = test_record_with_subrecords(b"TEST", &[]);
        record_bytes[8..12].copy_from_slice(&RecordFlags::DELETED.bits().to_le_bytes());
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let report = context.view(&record, false)?.validate();

        assert!(!report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::MissingRequired));
        Ok(())
    }

    fn test_subrecord_node(id: u32, path: &str, signature: [u8; 4], required: bool) -> SchemaNode {
        SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: path.to_owned(),
            name: path.to_owned(),
            required,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(signature),
                payload: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(id + 100),
                    path: format!("{path}/payload"),
                    name: "Payload".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Bytes { length: None },
                    },
                }),
            },
        }
    }

    struct TestRecordIndexResolver;

    impl crate::FormLinkResolver for TestRecordIndexResolver {
        fn resolve_form_id(
            &self,
            _source: crate::HandlerRecordContext,
            _form_id: bethkit_core::FormId,
            _targets: &[bethkit_core::Signature],
        ) -> Option<crate::FormLinkInfo> {
            None
        }

        fn resolve_record_index(
            &self,
            _source: crate::HandlerRecordContext,
            index: &str,
            key: &crate::RecordIndexKeyValue,
        ) -> Option<crate::IndexedRecordInfo> {
            matches!(
                (index, key),
                (
                    "complex_group",
                    crate::RecordIndexKeyValue::Text(value)
                ) if value == "EntryName"
            )
            .then(|| {
                crate::IndexedRecordInfo::new(
                    bethkit_core::FormId(0x1234),
                    crate::FormLinkInfo::new(
                        "Complex Entry [AVMD:00001234]",
                        "Complex Entry [AVMD:00001234]",
                    ),
                )
            })
        }
    }

    struct TestSnapNodeResolver;

    impl crate::FormLinkResolver for TestSnapNodeResolver {
        fn resolve_form_id(
            &self,
            _source: crate::HandlerRecordContext,
            _form_id: bethkit_core::FormId,
            _targets: &[bethkit_core::Signature],
        ) -> Option<crate::FormLinkInfo> {
            None
        }

        fn resolve_snap_node(
            &self,
            _source: crate::HandlerRecordContext,
            reference_form_id: Option<bethkit_core::FormId>,
            node_id: i64,
        ) -> Option<crate::ResolvedElementInfo> {
            (reference_form_id == Some(bethkit_core::FormId(0x2468)) && node_id == 7).then(|| {
                crate::ResolvedElementInfo::new(
                    bethkit_core::FormId(0x5678),
                    "STMP/2:Nodes/payload/element",
                    vec![3],
                    "[7] Second Node",
                    "Second Template [STMP:00005678]",
                )
            })
        }
    }

    struct TestNavmeshResolver;

    impl crate::FormLinkResolver for TestNavmeshResolver {
        fn resolve_form_id(
            &self,
            _source: crate::HandlerRecordContext,
            _form_id: bethkit_core::FormId,
            _targets: &[bethkit_core::Signature],
        ) -> Option<crate::FormLinkInfo> {
            None
        }

        fn source_load_order_form_id(&self, _source: crate::HandlerRecordContext) -> Option<u32> {
            Some(0x0100_1234)
        }

        fn resolve_navmesh(
            &self,
            _source: crate::HandlerRecordContext,
            form_id: bethkit_core::FormId,
        ) -> Option<crate::ResolvedNavmeshInfo> {
            (form_id == bethkit_core::FormId(0x2468)).then(|| {
                crate::ResolvedNavmeshInfo::new(
                    form_id,
                    0x0200_2468,
                    "Target Navmesh [NAVM:02002468]",
                    "NAVM/0:Navigation Mesh/payload/3:Triangles",
                    4,
                )
            })
        }
    }

    struct SourceRecordUnionSelector;

    impl crate::SemanticHandler for SourceRecordUnionSelector {
        fn id(&self) -> &'static str {
            "test.source_record_union"
        }

        fn version(&self) -> u32 {
            1
        }

        fn invoke(&self, invocation: crate::HandlerInvocation<'_>) -> Result<HandlerOutput> {
            if invocation.source_subrecord_index != Some(0) {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "test union selector received the wrong subrecord index".to_owned(),
                });
            }
            let record = invocation
                .source_record
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "test union selector requires the source record".to_owned(),
                })?;
            let has_data = record
                .subrecords()?
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"DATA"));
            Ok(HandlerOutput::Integer(i64::from(has_data)))
        }
    }

    struct ArrayIndexCount;

    impl crate::SemanticHandler for ArrayIndexCount {
        fn id(&self) -> &'static str {
            "test.array_index_count"
        }

        fn version(&self) -> u32 {
            1
        }

        fn invoke(&self, invocation: crate::HandlerInvocation<'_>) -> Result<HandlerOutput> {
            let index =
                invocation
                    .array_indices
                    .last()
                    .copied()
                    .ok_or_else(|| SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: "test array count requires an outer index".to_owned(),
                    })?;
            Ok(HandlerOutput::Integer((index + 1) as i64))
        }
    }

    fn terminated_byte_node() -> SchemaNode {
        SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/value".to_owned(),
            name: "Value".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Terminated {
                terminator: 0xff,
                child: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(2),
                    path: "TEST/value/body".to_owned(),
                    name: "Body".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
            },
        }
    }

    /// Includes and validates a structural terminator without consuming the following field.
    #[test]
    fn terminated_node_size_stops_after_terminator(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let node = terminated_byte_node();

        assert_eq!(node_data_size(&node, &[7, 0xff, 9], false)?, 2);
        assert!(node_data_size(&node, &[7, 0, 9], false).is_err());
        Ok(())
    }

    /// Uses a semantic array count without consuming bytes from the following field.
    #[test]
    fn callback_array_size_uses_resolved_element_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let node = SchemaNode {
            id: bethkit_schema::SchemaNodeId(3),
            path: "TEST/items".to_owned(),
            name: "Items".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(4),
                    path: "TEST/items/element".to_owned(),
                    name: "Element".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
                count: ArrayCount::Callback {
                    callback_id: "array.count".to_owned(),
                },
            },
        };
        let mut resolver = |resolved: &SchemaNode, count: &ArrayCount| {
            assert_eq!(resolved.path, "TEST/items");
            assert!(matches!(count, ArrayCount::Callback { .. }));
            Ok(3)
        };

        assert_eq!(
            node_data_size_with_resolver(&node, &[1, 2, 3, 9, 9], false, &mut resolver)?,
            3
        );
        Ok(())
    }

    /// Supplies enclosing array positions to nested dynamic-count callbacks.
    #[test]
    fn callback_array_count_receives_outer_array_index(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let inner_path = "TEST/0:Data/payload/element";
        let inner = SchemaNode {
            id: bethkit_schema::SchemaNodeId(3),
            path: inner_path.to_owned(),
            name: "Group".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(4),
                    path: format!("{inner_path}/element"),
                    name: "Value".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
                count: ArrayCount::Callback {
                    callback_id: "array.count".to_owned(),
                },
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Data".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: "TEST/0:Data/payload".to_owned(),
                            name: "Groups".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Array {
                                element: Box::new(inner),
                                count: ArrayCount::Fixed { count: 3 },
                            },
                        }),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "test.array_index_count".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: inner_path.to_owned(),
                callback_id: "array.count".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "test.array_index_count".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::Value::Null,
                    },
                },
            }],
        )?;
        let mut handlers = SemanticHandlerRegistry::new();
        handlers.register(Arc::new(ArrayIndexCount));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let record_bytes = test_record_bytes(b"TEST", b"DATA", &[1, 2, 3, 4, 5, 6]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let fields = context.view(&record, false)?.fields()?;

        let FieldValue::Array(groups) = &fields[0].value else {
            return Err("expected outer array".into());
        };
        assert_eq!(
            groups
                .iter()
                .map(|group| match group {
                    FieldValue::Array(values) => values.len(),
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        Ok(())
    }

    /// Splits Starfield star-slot payloads into five fixed outer groups.
    #[test]
    fn array_inclusion_callback_groups_remainder_elements(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let inner_path = "LGDI/0:Data/payload/element";
        let element = SchemaNode {
            id: bethkit_schema::SchemaNodeId(4),
            path: format!("{inner_path}/element"),
            name: "Entry".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(5),
                        path: format!("{inner_path}/element/0:Star Slot"),
                        name: "Star Slot".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
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
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(6),
                        path: format!("{inner_path}/element/1:Value"),
                        name: "Value".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::Integer {
                                integer: IntegerType {
                                    width: 1,
                                    signed: false,
                                    byte_order: ByteOrder::LittleEndian,
                                },
                            },
                        },
                    },
                ],
            },
        };
        let inner = SchemaNode {
            id: bethkit_schema::SchemaNodeId(3),
            path: inner_path.to_owned(),
            name: "Slot".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(element),
                count: ArrayCount::Remainder,
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "LGDI".to_owned(),
            name: "Leveled Item".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "LGDI/0:Data".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: "LGDI/0:Data/payload".to_owned(),
                            name: "Slots".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Array {
                                element: Box::new(inner),
                                count: ArrayCount::Fixed { count: 5 },
                            },
                        }),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.game = bethkit_schema::SchemaGame::Starfield;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "array.star_slot_matches_outer_index".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"LGDI"),
                name: "Leveled Item".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: inner_path.to_owned(),
                callback_id: "array.should_include".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "array.star_slot_matches_outer_index".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::Value::Null,
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut payload = Vec::new();
        for (slot, value) in [(0_u32, 10_u8), (0, 11), (2, 20), (4, 40)] {
            payload.extend(slot.to_le_bytes());
            payload.push(value);
        }
        let record_bytes = test_record_bytes(b"LGDI", b"DATA", &payload);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let fields = context.view(&record, false)?.fields()?;

        let FieldValue::Array(groups) = &fields[0].value else {
            return Err("expected star-slot groups".into());
        };
        assert_eq!(
            groups
                .iter()
                .map(|group| match group {
                    FieldValue::Array(values) => values.len(),
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            vec![2, 0, 1, 0, 1]
        );
        Ok(())
    }

    /// Reads a sibling counter before decoding a bounded array and its following field.
    #[test]
    fn record_view_resolves_prior_field_array_counts(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let byte = |id, path: &str, name: &str| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: path.to_owned(),
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Integer {
                    integer: IntegerType {
                        width: 1,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                },
            },
        };
        let count_path = "TEST/0:Data/payload/0:Count";
        let array_path = "TEST/0:Data/payload/1:Items";
        let payload = SchemaNode {
            id: bethkit_schema::SchemaNodeId(2),
            path: "TEST/0:Data/payload".to_owned(),
            name: "Payload".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    byte(3, count_path, "Count"),
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(4),
                        path: array_path.to_owned(),
                        name: "Items".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Array {
                            element: Box::new(byte(5, &format!("{array_path}/element"), "Item")),
                            count: ArrayCount::Expression {
                                expression: bethkit_schema::Expression::ReadField {
                                    path: count_path.to_owned(),
                                },
                            },
                        },
                    },
                    byte(6, "TEST/0:Data/payload/2:Tail", "Tail"),
                ],
            },
        };
        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: SchemaNode {
                    id: bethkit_schema::SchemaNodeId(0),
                    path: "TEST".to_owned(),
                    name: "Test".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![SchemaNode {
                            id: bethkit_schema::SchemaNodeId(1),
                            path: "TEST/0:Data".to_owned(),
                            name: "Data".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"DATA"),
                                payload: Box::new(payload),
                            },
                        }],
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let record_bytes = test_record_bytes(b"TEST", b"DATA", &[2, 10, 11, 99]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let fields = context.view(&record, false)?.fields()?;

        let FieldValue::Struct(values) = &fields[0].value else {
            return Err("expected decoded struct".into());
        };
        assert!(matches!(values[0].value, FieldValue::UInt(2)));
        assert!(matches!(
            &values[1].value,
            FieldValue::Array(items)
                if matches!(items.as_slice(), [FieldValue::UInt(10), FieldValue::UInt(11)])
        ));
        assert!(matches!(values[2].value, FieldValue::UInt(99)));
        Ok(())
    }

    /// Uses an earlier FormID field when selecting a later union variant.
    #[test]
    fn record_view_exposes_form_ids_to_field_expressions(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let form_id_path = "TEST/0:Data/payload/0:Parent Worldspace";
        let union_path = "TEST/0:Data/payload/1:Parent";
        let integer = |id, width| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: format!("{union_path}/variants/{id}"),
            name: format!("Variant {id}"),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Integer {
                    integer: IntegerType {
                        width,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                },
            },
        };
        let payload = SchemaNode {
            id: bethkit_schema::SchemaNodeId(2),
            path: "TEST/0:Data/payload".to_owned(),
            name: "Payload".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(3),
                        path: form_id_path.to_owned(),
                        name: "Parent Worldspace".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::FormId {
                                targets: Vec::new(),
                            },
                        },
                    },
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(4),
                        path: union_path.to_owned(),
                        name: "Parent".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Union {
                            selector: UnionSelector::Expression(Expression::Select {
                                condition: Box::new(Expression::Equal {
                                    left: Box::new(Expression::ReadField {
                                        path: form_id_path.to_owned(),
                                    }),
                                    right: Box::new(Expression::Int { value: 0 }),
                                }),
                                if_true: Box::new(Expression::Int { value: 1 }),
                                if_false: Box::new(Expression::Int { value: 0 }),
                            }),
                            variants: vec![integer(5, 1), integer(6, 2)],
                        },
                    },
                ],
            },
        };
        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: SchemaNode {
                    id: bethkit_schema::SchemaNodeId(0),
                    path: "TEST".to_owned(),
                    name: "Test".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![SchemaNode {
                            id: bethkit_schema::SchemaNodeId(1),
                            path: "TEST/0:Data".to_owned(),
                            name: "Data".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"DATA"),
                                payload: Box::new(payload),
                            },
                        }],
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let record_bytes = test_record_bytes(b"TEST", b"DATA", &[0, 0, 0, 0, 7, 0]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let fields = context.view(&record, false)?.fields()?;

        let FieldValue::Struct(values) = &fields[0].value else {
            return Err("expected decoded struct".into());
        };
        assert!(matches!(values[1].value, FieldValue::UInt(7)));
        Ok(())
    }

    /// Supplies the original record to callback-selected unions while decoding.
    #[test]
    fn callback_union_decoding_receives_source_record(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let integer = |id, width| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: format!("TEST/0:Data/payload/variants/{id}"),
            name: format!("Variant {id}"),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Integer {
                    integer: IntegerType {
                        width,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                },
            },
        };
        let union_path = "TEST/0:Data/payload";
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Data".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: union_path.to_owned(),
                            name: "Payload".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Union {
                                selector: UnionSelector::Callback {
                                    callback_id: "union.select".to_owned(),
                                },
                                variants: vec![integer(3, 1), integer(4, 2)],
                            },
                        }),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "test.source_record_union".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: union_path.to_owned(),
                callback_id: "union.select".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "test.source_record_union".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::Value::Null,
                    },
                },
            }],
        )?;
        let mut handlers = SemanticHandlerRegistry::new();
        handlers.register(Arc::new(SourceRecordUnionSelector));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let record_bytes = test_record_bytes(b"TEST", b"DATA", &[7, 0]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        let fields = context.view(&record, false)?.fields()?;

        assert!(matches!(fields[0].value, FieldValue::UInt(7)));
        Ok(())
    }

    /// Supplies already decoded sibling strings to callback-selected unions.
    #[test]
    fn callback_union_decoding_receives_sibling_scope(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let script_name_path = "TEST/0:Data/payload/0:ScriptName";
        let union_path = "TEST/0:Data/payload/1:Script";
        let payload = SchemaNode {
            id: bethkit_schema::SchemaNodeId(2),
            path: "TEST/0:Data/payload".to_owned(),
            name: "Payload".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(3),
                        path: script_name_path.to_owned(),
                        name: "ScriptName".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::String {
                                string: StringType {
                                    encoding: "utf8".to_owned(),
                                    localized: false,
                                    zero_terminated: false,
                                    fixed_length: None,
                                    length_prefix: Some(StringLengthPrefix {
                                        width: 1,
                                        offset: 1,
                                    }),
                                    trailing_terminator: None,
                                    allowed_values: Vec::new(),
                                },
                            },
                        },
                    },
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(4),
                        path: union_path.to_owned(),
                        name: "Script".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Union {
                            selector: UnionSelector::Callback {
                                callback_id: "union.select".to_owned(),
                            },
                            variants: vec![
                                SchemaNode {
                                    id: bethkit_schema::SchemaNodeId(5),
                                    path: format!("{union_path}/variants/0:Data"),
                                    name: "Data".to_owned(),
                                    required: true,
                                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: Some(1) },
                                    },
                                },
                                SchemaNode {
                                    id: bethkit_schema::SchemaNodeId(6),
                                    path: format!("{union_path}/variants/1:Empty"),
                                    name: "Empty".to_owned(),
                                    required: true,
                                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Unused { length: 0 },
                                    },
                                },
                            ],
                        },
                    },
                ],
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Data".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(payload),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "select.empty_string".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: union_path.to_owned(),
                callback_id: "union.select".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "select.empty_string".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({ "path": script_name_path }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;

        for (payload, expected_name, expected_bytes, expected_variant) in [
            (&[0_u8][..], "", &[][..], "variants/1:Empty"),
            (&[1_u8, b'Q', 7][..], "Q", &[7_u8][..], "variants/0:Data"),
        ] {
            let record_bytes = test_record_bytes(b"TEST", b"DATA", payload);
            let mut cursor = SliceCursor::new(&record_bytes);
            let record = Record::parse_header(&mut cursor, &GameContext::sse())?;
            let fields = context.view(&record, false)?.fields()?;
            let FieldValue::Struct(values) = &fields[0].value else {
                return Err("expected decoded sibling scope".into());
            };
            assert!(matches!(
                &values[0].value,
                FieldValue::String(value) if value == expected_name
            ));
            assert!(matches!(
                &values[1].value,
                FieldValue::Bytes(value) if value.as_ref() == expected_bytes
            ));
            assert_eq!(
                values[1].effective_path.as_deref(),
                Some(format!("{union_path}/{expected_variant}").as_str())
            );
        }
        Ok(())
    }

    /// Reconstructs repeated structural callback sites from grammar repeat scopes.
    #[test]
    fn repeated_structures_group_subrecords_by_occurrence(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        fn byte_subrecord(
            id: u32,
            path: &str,
            name: &str,
            signature: [u8; 4],
            required: bool,
        ) -> SchemaNode {
            SchemaNode {
                id: bethkit_schema::SchemaNodeId(id),
                path: path.to_owned(),
                name: name.to_owned(),
                required,
                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Subrecord {
                    signature: SchemaSignature(signature),
                    payload: Box::new(SchemaNode {
                        id: bethkit_schema::SchemaNodeId(id + 1),
                        path: format!("{path}/payload"),
                        name: "Byte".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::Integer {
                                integer: IntegerType {
                                    width: 1,
                                    signed: false,
                                    byte_order: ByteOrder::LittleEndian,
                                },
                            },
                        },
                    }),
                },
            }
        }

        let structure_path = "TEST/0:Conditions/repeat/0:Condition";
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Conditions".to_owned(),
                    name: "Conditions".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Repeat {
                        minimum: 0,
                        maximum: None,
                        child: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: structure_path.to_owned(),
                            name: "Condition".to_owned(),
                            required: false,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Sequence {
                                children: vec![
                                    byte_subrecord(
                                        3,
                                        &format!("{structure_path}/0:CTDA"),
                                        "CTDA",
                                        *b"CTDA",
                                        true,
                                    ),
                                    byte_subrecord(
                                        5,
                                        &format!("{structure_path}/1:Parameter"),
                                        "Parameter",
                                        *b"CIS1",
                                        false,
                                    ),
                                ],
                            },
                        }),
                    },
                }],
            },
        };
        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let record_bytes = test_record_with_subrecords(
            b"TEST",
            &[(b"CTDA", &[1]), (b"CIS1", &[2]), (b"CTDA", &[3])],
        );
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;

        // when
        let structures = context
            .view(&record, false)?
            .repeated_structures(structure_path)?;

        // then
        assert_eq!(structures.len(), 2);
        let FieldValue::Struct(first) = &structures[0] else {
            return Err("expected first structural occurrence".into());
        };
        let FieldValue::Struct(second) = &structures[1] else {
            return Err("expected second structural occurrence".into());
        };
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 1);
        assert!(matches!(first[0].value, FieldValue::UInt(1)));
        assert!(matches!(first[1].value, FieldValue::UInt(2)));
        assert!(matches!(second[0].value, FieldValue::UInt(3)));
        assert_eq!(
            context
                .view(&record, false)?
                .repeated_structures(&format!("{structure_path}/payload"))?
                .len(),
            2
        );
        Ok(())
    }

    /// Formats repeated CTDA structures with child callbacks and logical connectors.
    #[test]
    fn repeated_condition_summary_formats_complete_structure(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let structure_path = "TEST/0:Conditions/repeat/0:Condition";
        let ctda_path = format!("{structure_path}/0:CTDA");
        let payload_path = format!("{ctda_path}/payload");
        let field = |id: u32, index: usize, name: &str, primitive: PrimitiveType| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: format!("{payload_path}/{index}:{name}"),
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive { primitive },
        };
        let integer = |width| PrimitiveType::Integer {
            integer: IntegerType {
                width,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
        };
        let payload = SchemaNode {
            id: bethkit_schema::SchemaNodeId(4),
            path: payload_path.clone(),
            name: "Condition Data".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    field(5, 0, "Type", integer(1)),
                    field(
                        6,
                        1,
                        "Comparison Value",
                        PrimitiveType::Float {
                            width: 4,
                            byte_order: ByteOrder::LittleEndian,
                            scale: 1.0,
                            digits: 6,
                        },
                    ),
                    field(7, 2, "Function", integer(2)),
                ],
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Conditions".to_owned(),
                    name: "Conditions".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Repeat {
                        minimum: 0,
                        maximum: None,
                        child: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: structure_path.to_owned(),
                            name: "Condition".to_owned(),
                            required: false,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Sequence {
                                children: vec![SchemaNode {
                                    id: bethkit_schema::SchemaNodeId(3),
                                    path: ctda_path,
                                    name: "CTDA".to_owned(),
                                    required: true,
                                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Subrecord {
                                        signature: SchemaSignature(*b"CTDA"),
                                        payload: Box::new(payload),
                                    },
                                }],
                            },
                        }),
                    },
                }],
            },
        };
        let function_path = format!("{payload_path}/2:Function");
        let bindings = vec![
            CallbackBinding {
                path: structure_path.to_owned(),
                callback_id: "def.value_transform".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "format.ctda_condition".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            },
            CallbackBinding {
                path: function_path,
                callback_id: "integer.formatter".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "11".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "format.ctda_function".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            },
        ];
        let mut manifest = test_manifest();
        manifest.callbacks_total = 2;
        manifest.callbacks_classified = 2;
        manifest.required_handlers = vec![
            HandlerRequirement {
                id: "format.ctda_condition".to_owned(),
                minimum_version: 1,
            },
            HandlerRequirement {
                id: "format.ctda_function".to_owned(),
                minimum_version: 1,
            },
        ];
        let table = bethkit_schema::ConditionFunctionTable::new(
            None,
            None,
            vec![bethkit_schema::ConditionFunction::new(
                1,
                "GetDistance",
                "",
                [0, 0, 0],
                [false, false, false],
            )],
        );
        let package = SchemaPackage::new_with_semantics(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            bindings,
            Some(table),
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut first = vec![0_u8];
        first.extend_from_slice(&1.25_f32.to_le_bytes());
        first.extend_from_slice(&1_u16.to_le_bytes());
        let mut second = vec![0_u8];
        second.extend_from_slice(&2.0_f32.to_le_bytes());
        second.extend_from_slice(&1_u16.to_le_bytes());
        let record_bytes =
            test_record_with_subrecords(b"TEST", &[(b"CTDA", &first), (b"CTDA", &second)]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::sse())?;
        let view = context.view(&record, false)?;

        // when / then
        assert_eq!(
            view.format_repeated_structure_as(structure_path, 0, ValueFormat::Summary)?,
            Some("Subject.GetDistance = 1.25 AND".to_owned())
        );
        assert_eq!(
            view.format_repeated_structure_as(structure_path, 1, ValueFormat::Summary)?,
            Some("Subject.GetDistance = 2".to_owned())
        );
        Ok(())
    }

    /// Resolves nested blueprint component links through the decoded record structure.
    #[test]
    fn blueprint_component_callbacks_receive_structural_record_scope(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let repeat_path = "TEST/0:Components/repeat/0:Component";
        let item_path = format!("{repeat_path}/0:Blue Print Components/payload/element");
        let slot_path = format!("{repeat_path}/1:Ship Weapon Binding/payload/0:Weapon Slot 1");
        let primitive = |id: u32, path: String, name: &str, primitive: PrimitiveType| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path,
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive { primitive },
        };
        let integer = |width: u8, signed: bool| PrimitiveType::Integer {
            integer: IntegerType {
                width,
                signed,
                byte_order: ByteOrder::LittleEndian,
            },
        };
        let vector = |id: u32, path: String, name: &str, scale: f64, digits: i32| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: path.clone(),
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: ["X", "Y", "Z"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        primitive(
                            id + 1 + u32::try_from(index).expect("test vector index fits u32"),
                            format!("{path}/{index}:{name}"),
                            name,
                            PrimitiveType::Float {
                                width: 4,
                                byte_order: ByteOrder::LittleEndian,
                                scale,
                                digits,
                            },
                        )
                    })
                    .collect(),
            },
        };
        let position_rotation_path = format!("{item_path}/2:Position/Rotation");
        let item = SchemaNode {
            id: bethkit_schema::SchemaNodeId(10),
            path: item_path.clone(),
            name: "Item".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    primitive(
                        11,
                        format!("{item_path}/0:Base Item"),
                        "Base Item",
                        PrimitiveType::FormId {
                            targets: vec![SchemaSignature(*b"GBFM")],
                        },
                    ),
                    primitive(
                        12,
                        format!("{item_path}/1:Construction Object"),
                        "Construction Object",
                        PrimitiveType::FormId {
                            targets: vec![SchemaSignature(*b"COBJ")],
                        },
                    ),
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(13),
                        path: position_rotation_path.clone(),
                        name: "Position/Rotation".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Struct {
                            fields: vec![
                                vector(
                                    14,
                                    format!("{position_rotation_path}/0:Position"),
                                    "Position",
                                    1.0,
                                    6,
                                ),
                                vector(
                                    18,
                                    format!("{position_rotation_path}/1:Rotation"),
                                    "Rotation",
                                    57.295_779_513_082_3,
                                    4,
                                ),
                            ],
                        },
                    },
                    primitive(
                        22,
                        format!("{item_path}/3:Part ID"),
                        "Part ID",
                        integer(4, false),
                    ),
                ],
            },
        };
        let blueprint_path = format!("{repeat_path}/0:Blue Print Components");
        let blueprint = SchemaNode {
            id: bethkit_schema::SchemaNodeId(8),
            path: blueprint_path.clone(),
            name: "Blue Print Components".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(*b"BUO4"),
                payload: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(9),
                    path: format!("{blueprint_path}/payload"),
                    name: "Blue Print Components".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Array {
                        element: Box::new(item),
                        count: ArrayCount::Remainder,
                    },
                }),
            },
        };
        let ship_path = format!("{repeat_path}/1:Ship Weapon Binding");
        let ship = SchemaNode {
            id: bethkit_schema::SchemaNodeId(23),
            path: ship_path.clone(),
            name: "Ship Weapon Binding".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(*b"SHWB"),
                payload: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(24),
                    path: format!("{ship_path}/payload"),
                    name: "Ship Weapon Binding".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Struct {
                        fields: (0_u32..3)
                            .map(|index| {
                                primitive(
                                    25 + index,
                                    format!(
                                        "{ship_path}/payload/{index}:Weapon Slot {}",
                                        index + 1
                                    ),
                                    &format!("Weapon Slot {}", index + 1),
                                    integer(4, true),
                                )
                            })
                            .collect(),
                    },
                }),
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "TEST/0:Components".to_owned(),
                    name: "Components".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Repeat {
                        minimum: 1,
                        maximum: None,
                        child: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: repeat_path.to_owned(),
                            name: "Component".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Sequence {
                                children: vec![blueprint, ship],
                            },
                        }),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.game = bethkit_schema::SchemaGame::Starfield;
        manifest.callbacks_total = 2;
        manifest.callbacks_classified = 2;
        manifest.required_handlers = vec![
            HandlerRequirement {
                id: "format.blueprint_component_summary".to_owned(),
                minimum_version: 1,
            },
            HandlerRequirement {
                id: "resolve.blueprint_component".to_owned(),
                minimum_version: 1,
            },
        ];
        let bindings = vec![
            CallbackBinding {
                path: slot_path.clone(),
                callback_id: "def.value_transform".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "format.blueprint_component_summary".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            },
            CallbackBinding {
                path: slot_path.clone(),
                callback_id: "value.links_to".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "11".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "resolve.blueprint_component".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            },
        ];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            bindings,
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut blueprint_data = Vec::new();
        blueprint_data.extend_from_slice(&0x1234_u32.to_le_bytes());
        blueprint_data.extend_from_slice(&0_u32.to_le_bytes());
        for value in [
            1.0_f32,
            2.0,
            3.0,
            std::f32::consts::FRAC_PI_2,
            0.0,
            -std::f32::consts::FRAC_PI_4,
        ] {
            blueprint_data.extend_from_slice(&value.to_le_bytes());
        }
        blueprint_data.extend_from_slice(&7_u32.to_le_bytes());
        let mut ship_data = Vec::new();
        for value in [7_i32, -1, -1] {
            ship_data.extend_from_slice(&value.to_le_bytes());
        }
        let record_bytes = test_record_with_subrecords(
            b"TEST",
            &[(b"BUO4", &blueprint_data), (b"SHWB", &ship_data)],
        );
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::starfield())?;
        let view = context.view(&record, false)?;
        let fields = view.fields()?;
        let FieldValue::Struct(slots) = &fields[1].value else {
            return Err("expected decoded ship weapon slots".into());
        };

        // when / then
        assert_eq!(
            view.format_value_as(&slot_path, &slots[0].value, ValueFormat::Display)?,
            Some("[7] 00001234 Pos:(1, 2, 3) Rot:(90, 0, -45)".to_owned())
        );
        assert!(matches!(
            view.resolve_link(&slot_path, &slots[0].value)?,
            Some(SemanticLink::Element {
                path,
                array_indices,
            }) if path == item_path && array_indices == vec![0, 0]
        ));
        Ok(())
    }

    /// Keeps optional AVMD value fields local to their selected entry occurrence.
    #[test]
    fn repeated_field_callbacks_receive_active_occurrence_scope(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let type_path = "AVMD/0:Type";
        let entry_path = "AVMD/1:Entries/repeat/0:Entry";
        let name_path = format!("{entry_path}/0:Name");
        let name_payload_path = format!("{name_path}/payload");
        let value_path = format!("{entry_path}/1:Value");
        let string_subrecord =
            |id: u32, path: String, name: &str, signature: [u8; 4], required: bool| SchemaNode {
                id: bethkit_schema::SchemaNodeId(id),
                path: path.clone(),
                name: name.to_owned(),
                required,
                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Subrecord {
                    signature: SchemaSignature(signature),
                    payload: Box::new(SchemaNode {
                        id: bethkit_schema::SchemaNodeId(id + 1),
                        path: format!("{path}/payload"),
                        name: "String".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::String {
                                string: StringType {
                                    encoding: "windows_1252".to_owned(),
                                    localized: false,
                                    zero_terminated: true,
                                    fixed_length: None,
                                    length_prefix: None,
                                    trailing_terminator: None,
                                    allowed_values: Vec::new(),
                                },
                            },
                        },
                    }),
                },
            };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "AVMD".to_owned(),
            name: "AVM Data".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(1),
                        path: type_path.to_owned(),
                        name: "Type".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Subrecord {
                            signature: SchemaSignature(*b"MNAM"),
                            payload: Box::new(SchemaNode {
                                id: bethkit_schema::SchemaNodeId(2),
                                path: format!("{type_path}/payload"),
                                name: "Type".to_owned(),
                                required: true,
                                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Primitive {
                                    primitive: PrimitiveType::Enumeration {
                                        integer: IntegerType {
                                            width: 4,
                                            signed: false,
                                            byte_order: ByteOrder::LittleEndian,
                                        },
                                        values: vec![(2, "Complex Group".to_owned())],
                                    },
                                },
                            }),
                        },
                    },
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(3),
                        path: "AVMD/1:Entries".to_owned(),
                        name: "Entries".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Repeat {
                            minimum: 1,
                            maximum: None,
                            child: Box::new(SchemaNode {
                                id: bethkit_schema::SchemaNodeId(4),
                                path: entry_path.to_owned(),
                                name: "Entry".to_owned(),
                                required: true,
                                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Sequence {
                                    children: vec![
                                        string_subrecord(5, name_path, "Name", *b"LNAM", true),
                                        string_subrecord(
                                            7,
                                            value_path.clone(),
                                            "Value",
                                            *b"VNAM",
                                            false,
                                        ),
                                    ],
                                },
                            }),
                        },
                    },
                ],
            },
        };
        let bindings = vec![
            CallbackBinding {
                path: name_payload_path.clone(),
                callback_id: "def.value_transform".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "format.avmd_entry_reference".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "mode": "name",
                            "type_path": type_path,
                            "value_path": value_path
                        }),
                    },
                },
            },
            CallbackBinding {
                path: name_payload_path.clone(),
                callback_id: "value.links_to".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "11".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "resolve.avmd_entry_reference".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "mode": "name",
                            "type_path": type_path,
                            "value_path": value_path
                        }),
                    },
                },
            },
        ];
        let mut manifest = test_manifest();
        manifest.game = bethkit_schema::SchemaGame::Starfield;
        manifest.callbacks_total = 2;
        manifest.callbacks_classified = 2;
        manifest.required_handlers = vec![
            HandlerRequirement {
                id: "format.avmd_entry_reference".to_owned(),
                minimum_version: 1,
            },
            HandlerRequirement {
                id: "resolve.avmd_entry_reference".to_owned(),
                minimum_version: 1,
            },
        ];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"AVMD"),
                name: "AVM Data".to_owned(),
                root,
            }],
            bindings,
        )?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestRecordIndexResolver));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let record_bytes = test_record_with_subrecords(
            b"AVMD",
            &[
                (b"MNAM", &[2, 0, 0, 0]),
                (b"LNAM", b"EntryName\0"),
                (b"LNAM", b"EntryName\0"),
                (b"VNAM", b"ExplicitValue\0"),
            ],
        );
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::starfield())?;
        let view = context.view(&record, false)?;

        // when / then
        assert_eq!(
            view.format_repeated_field_as(
                entry_path,
                0,
                &name_payload_path,
                ValueFormat::Display,
            )?,
            Some("Complex Entry [AVMD:00001234]".to_owned())
        );
        assert!(matches!(
            view.resolve_repeated_field_link(entry_path, 0, &name_payload_path)?,
            Some(SemanticLink::Record {
                form_id: bethkit_core::FormId(0x1234),
            })
        ));
        assert_eq!(
            view.format_repeated_field_as(
                entry_path,
                1,
                &name_payload_path,
                ValueFormat::Display,
            )?,
            None
        );
        Ok(())
    }

    /// Resolves a snap node through the sibling reference in the selected repeat occurrence.
    #[test]
    fn snap_node_callbacks_use_active_repeat_reference(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let structure_path = "REFR/25:Snap Links/payload/element";
        let reference_path = "REFR/25:Snap Links/payload/element/0:Linked Reference";
        let links_path = "REFR/25:Snap Links/payload/element/1:Links";
        let parent_path = "REFR/25:Snap Links/payload/element/1:Links/element/0:Parent Node";
        let linked_path = "REFR/25:Snap Links/payload/element/1:Links/element/1:Linked Node";
        let integer = PrimitiveType::Integer {
            integer: IntegerType {
                width: 4,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
        };
        let primitive = |id: u32, path: &str, name: &str, primitive: PrimitiveType| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path: path.to_owned(),
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive { primitive },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "REFR".to_owned(),
            name: "Placed Object".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "REFR/25:Snap Links".to_owned(),
                    name: "Snap Links".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Repeat {
                        minimum: 0,
                        maximum: None,
                        child: Box::new(SchemaNode {
                            id: bethkit_schema::SchemaNodeId(2),
                            path: structure_path.to_owned(),
                            name: "Snap Link".to_owned(),
                            required: true,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"XSL1"),
                                payload: Box::new(SchemaNode {
                                    id: bethkit_schema::SchemaNodeId(3),
                                    path: format!("{structure_path}/payload"),
                                    name: "Snap Link".to_owned(),
                                    required: true,
                                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Struct {
                                        fields: vec![
                                            primitive(
                                                4,
                                                reference_path,
                                                "Linked Reference",
                                                PrimitiveType::FormId {
                                                    targets: vec![SchemaSignature(*b"REFR")],
                                                },
                                            ),
                                            SchemaNode {
                                                id: bethkit_schema::SchemaNodeId(5),
                                                path: links_path.to_owned(),
                                                name: "Links".to_owned(),
                                                required: true,
                                                conflict_priority:
                                                    bethkit_schema::ConflictPriority::Normal,
                                                condition: None,
                                                kind: SchemaNodeKind::Struct {
                                                    fields: vec![
                                                        primitive(
                                                            6,
                                                            parent_path,
                                                            "Parent Node",
                                                            integer.clone(),
                                                        ),
                                                        primitive(
                                                            7,
                                                            linked_path,
                                                            "Linked Node",
                                                            integer,
                                                        ),
                                                    ],
                                                },
                                            },
                                        ],
                                    },
                                }),
                            },
                        }),
                    },
                }],
            },
        };
        let bindings = [
            ("def.value_transform", "format.snap_node_summary"),
            ("value.links_to", "resolve.snap_node"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (callback_id, handler))| CallbackBinding {
            path: linked_path.to_owned(),
            callback_id: callback_id.to_owned(),
            callback_slot: None,
            implementation_fingerprint: format!("{index:064x}"),
            implementation: CallbackImplementation::BuiltIn {
                operation: BuiltInOperation {
                    id: handler.to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        })
        .collect();
        let mut manifest = test_manifest();
        manifest.game = bethkit_schema::SchemaGame::Starfield;
        manifest.callbacks_total = 2;
        manifest.callbacks_classified = 2;
        manifest.required_handlers = vec![
            HandlerRequirement {
                id: "format.snap_node_summary".to_owned(),
                minimum_version: 1,
            },
            HandlerRequirement {
                id: "resolve.snap_node".to_owned(),
                minimum_version: 1,
            },
        ];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"REFR"),
                name: "Placed Object".to_owned(),
                root,
            }],
            bindings,
        )?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestSnapNodeResolver));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let first = [0x11, 0x11, 0, 0, 7, 0, 0, 0, 7, 0, 0, 0];
        let second = [0x68, 0x24, 0, 0, 7, 0, 0, 0, 7, 0, 0, 0];
        let record_bytes =
            test_record_with_subrecords(b"REFR", &[(b"XSL1", &first), (b"XSL1", &second)]);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::starfield())?;
        let view = context.view(&record, false)?;

        // when / then
        assert_eq!(
            view.format_repeated_field_as(structure_path, 0, linked_path, ValueFormat::Display,)?,
            None
        );
        assert_eq!(
            view.format_repeated_field_as(structure_path, 1, linked_path, ValueFormat::Display,)?,
            Some("[7] Second Node on Second Template [STMP:00005678]".to_owned())
        );
        assert!(matches!(
            view.resolve_repeated_field_link(structure_path, 1, linked_path)?,
            Some(SemanticLink::ExternalElement {
                record_form_id: bethkit_core::FormId(0x5678),
                path,
                array_indices,
            }) if path == "STMP/2:Nodes/payload/element" && array_indices == vec![3]
        ));
        Ok(())
    }

    /// Routes selected triangle positions through navigation-mesh edge callbacks.
    #[test]
    fn array_element_edge_callbacks_receive_selected_triangle_index(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let payload_path = "NAVM/0:Navigation Mesh/payload";
        let triangles_path = format!("{payload_path}/3:Triangles");
        let edge_path = format!("{triangles_path}/element/3:Edge 0-1");
        let edge_links_path = format!("{payload_path}/4:Edge Links");
        let integer = |width: u8, signed: bool| PrimitiveType::Integer {
            integer: IntegerType {
                width,
                signed,
                byte_order: ByteOrder::LittleEndian,
            },
        };
        let primitive = |id: u32, path: String, name: &str, value: PrimitiveType| SchemaNode {
            id: bethkit_schema::SchemaNodeId(id),
            path,
            name: name.to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive { primitive: value },
        };
        let triangle = SchemaNode {
            id: bethkit_schema::SchemaNodeId(4),
            path: format!("{triangles_path}/element"),
            name: "Triangle".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    primitive(
                        5,
                        format!("{triangles_path}/element/0:Flags"),
                        "Flags",
                        integer(2, false),
                    ),
                    primitive(6, edge_path.clone(), "Edge 0-1", integer(2, true)),
                ],
            },
        };
        let edge_link = SchemaNode {
            id: bethkit_schema::SchemaNodeId(8),
            path: format!("{edge_links_path}/element"),
            name: "Edge Link".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    primitive(
                        9,
                        format!("{edge_links_path}/element/1:Mesh"),
                        "Mesh",
                        PrimitiveType::FormId {
                            targets: vec![SchemaSignature(*b"NAVM")],
                        },
                    ),
                    primitive(
                        10,
                        format!("{edge_links_path}/element/2:Triangle Index"),
                        "Triangle Index",
                        integer(2, false),
                    ),
                ],
            },
        };
        let payload = SchemaNode {
            id: bethkit_schema::SchemaNodeId(2),
            path: payload_path.to_owned(),
            name: "Navigation Mesh".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(3),
                        path: triangles_path.clone(),
                        name: "Triangles".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Array {
                            element: Box::new(triangle),
                            count: ArrayCount::Fixed { count: 2 },
                        },
                    },
                    SchemaNode {
                        id: bethkit_schema::SchemaNodeId(7),
                        path: edge_links_path,
                        name: "Edge Links".to_owned(),
                        required: true,
                        conflict_priority: bethkit_schema::ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Array {
                            element: Box::new(edge_link),
                            count: ArrayCount::Fixed { count: 1 },
                        },
                    },
                ],
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "NAVM".to_owned(),
            name: "Navigation Mesh".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: bethkit_schema::SchemaNodeId(1),
                    path: "NAVM/0:Navigation Mesh".to_owned(),
                    name: "Navigation Mesh".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"NVNM"),
                        payload: Box::new(payload),
                    },
                }],
            },
        };
        let bindings = [
            ("integer.formatter", "format.navmesh_edge"),
            ("value.links_to", "resolve.navmesh_edge"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (callback_id, handler))| CallbackBinding {
            path: edge_path.clone(),
            callback_id: callback_id.to_owned(),
            callback_slot: None,
            implementation_fingerprint: format!("{index:064x}"),
            implementation: CallbackImplementation::BuiltIn {
                operation: BuiltInOperation {
                    id: handler.to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        })
        .collect();
        let mut manifest = test_manifest();
        manifest.game = bethkit_schema::SchemaGame::Fallout4;
        manifest.callbacks_total = 2;
        manifest.callbacks_classified = 2;
        manifest.required_handlers = vec![
            HandlerRequirement {
                id: "format.navmesh_edge".to_owned(),
                minimum_version: 1,
            },
            HandlerRequirement {
                id: "resolve.navmesh_edge".to_owned(),
                minimum_version: 1,
            },
        ];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"NAVM"),
                name: "Navigation Mesh".to_owned(),
                root,
            }],
            bindings,
        )?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestNavmeshResolver));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let payload = [0, 0, 0, 0, 1, 0, 0, 0, 0x68, 0x24, 0, 0, 2, 0];
        let record_bytes = test_record_bytes(b"NAVM", b"NVNM", &payload);
        let mut cursor = SliceCursor::new(&record_bytes);
        let record = Record::parse_header(&mut cursor, &GameContext::fallout4())?;
        let view = context.view(&record, false)?;

        // when / then
        assert_eq!(
            view.format_array_element_field_as(
                &triangles_path,
                0,
                &edge_path,
                ValueFormat::Display,
            )?,
            Some("0".to_owned())
        );
        assert_eq!(
            view.format_array_element_field_as(
                &triangles_path,
                1,
                &edge_path,
                ValueFormat::Display,
            )?,
            Some("0 (#2 in Target Navmesh [NAVM:02002468])".to_owned())
        );
        assert!(matches!(
            view.resolve_array_element_field_link(&triangles_path, 0, &edge_path)?,
            Some(SemanticLink::Element {
                path,
                array_indices,
            }) if path == format!("{triangles_path}/element") && array_indices == vec![0]
        ));
        assert!(matches!(
            view.resolve_array_element_field_link(&triangles_path, 1, &edge_path)?,
            Some(SemanticLink::ExternalElement {
                record_form_id: bethkit_core::FormId(0x2468),
                path,
                array_indices,
            }) if path == "NAVM/0:Navigation Mesh/payload/3:Triangles/element"
                && array_indices == vec![2]
        ));
        Ok(())
    }

    fn test_record_bytes(
        record_signature: &[u8; 4],
        subrecord_signature: &[u8; 4],
        payload: &[u8],
    ) -> Vec<u8> {
        test_record_with_subrecords(record_signature, &[(subrecord_signature, payload)])
    }

    fn test_record_with_subrecords(
        record_signature: &[u8; 4],
        subrecords: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        let data_size = subrecords.iter().fold(0_u32, |size, (_, payload)| {
            size + 6 + u32::try_from(payload.len()).expect("test payload fits u32")
        });
        let mut bytes = Vec::new();
        bytes.extend_from_slice(record_signature);
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&44_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        for (signature, payload) in subrecords {
            bytes.extend_from_slice(*signature);
            bytes.extend_from_slice(
                &u16::try_from(payload.len())
                    .expect("test payload fits u16")
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(payload);
        }
        bytes
    }

    fn test_manifest() -> SchemaManifest {
        SchemaManifest {
            format_version: PACKAGE_FORMAT_VERSION,
            game: bethkit_schema::SchemaGame::SkyrimSe,
            package_version: "test".to_owned(),
            source_repository: "TES5Edit/TES5Edit".to_owned(),
            source_tag: "test".to_owned(),
            source_commit: "00".repeat(20),
            source_archive_sha256: "00".repeat(32),
            exporter_version: "test".to_owned(),
            exporter_binary_sha256: "00".repeat(32),
            exporter_map_sha256: "00".repeat(32),
            exporter_patch_sha256: "00".repeat(32),
            exporter_build_sha256: "00".repeat(32),
            conversion_rules_sha256: "00".repeat(32),
            minimum_bethkit_version: "0.4.0".to_owned(),
            minimum_abi_version: 2,
            validation_status: ValidationStatus::Candidate,
            corpus_sha256: "00".repeat(32),
            validated_records: 0,
            byte_coverage: 0.0,
            callbacks_total: 0,
            callbacks_classified: 0,
            required_decoders: Vec::new(),
            required_handlers: Vec::new(),
        }
    }

    /// Matches xEdit's packed 6/14/30-bit unsigned counter decoding.
    #[test]
    fn packed_unsigned_decodes_all_widths() -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(decode_packed_unsigned(&[0xfc], "TEST")?, (63, 1));
        assert_eq!(decode_packed_unsigned(&[0x01, 0x01], "TEST")?, (64, 2));
        assert_eq!(
            decode_packed_unsigned(&[0x02, 0x00, 0x01, 0x00], "TEST")?,
            (16_384, 4)
        );
        assert!(decode_packed_unsigned(&[0x01], "TEST").is_err());
        Ok(())
    }

    /// Squares packed matrix dimensions before walking their elements.
    #[test]
    fn packed_matrix_prefix_counts_squared_elements(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let node = SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/matrix".to_owned(),
            name: "Matrix".to_owned(),
            required: false,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(2),
                    path: "TEST/matrix/element".to_owned(),
                    name: "Element".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
                count: ArrayCount::PackedPrefixed {
                    square: true,
                    terminator: None,
                },
            },
        };

        assert_eq!(node_data_size(&node, &[8, 1, 2, 3, 4, 99], false)?, 5);
        Ok(())
    }

    /// Counts a prefixed array without consuming bytes from the following struct field.
    #[test]
    fn prefixed_array_size_includes_counter_and_elements(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let integer = IntegerType {
            width: 2,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
        };
        let node = SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/items".to_owned(),
            name: "Items".to_owned(),
            required: false,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(2),
                    path: "TEST/items/element".to_owned(),
                    name: "Item".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer { integer },
                    },
                }),
                count: ArrayCount::Prefixed {
                    integer: IntegerType {
                        width: 1,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                    terminator: Some(0x7c),
                },
            },
        };

        assert_eq!(
            node_data_size(&node, b"\x02\x7c\x01\0\x02\0tail", false)?,
            6
        );
        assert!(node_data_size(&node, b"\x02\x00\x01\0\x02\0tail", false).is_err());
        Ok(())
    }

    /// Walks length-prefixed variable elements instead of consuming the tail.
    #[test]
    fn prefixed_array_size_supports_variable_struct_elements(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let byte_order = ByteOrder::LittleEndian;
        let node = SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/items".to_owned(),
            name: "Items".to_owned(),
            required: false,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: bethkit_schema::SchemaNodeId(2),
                    path: "TEST/items/element".to_owned(),
                    name: "Item".to_owned(),
                    required: false,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Struct {
                        fields: vec![
                            SchemaNode {
                                id: bethkit_schema::SchemaNodeId(3),
                                path: "TEST/items/element/name".to_owned(),
                                name: "Name".to_owned(),
                                required: true,
                                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Primitive {
                                    primitive: PrimitiveType::String {
                                        string: StringType {
                                            encoding: "utf8".to_owned(),
                                            localized: false,
                                            zero_terminated: false,
                                            fixed_length: None,
                                            length_prefix: Some(
                                                bethkit_schema::StringLengthPrefix {
                                                    width: 1,
                                                    offset: 1,
                                                },
                                            ),
                                            trailing_terminator: None,
                                            allowed_values: Vec::new(),
                                        },
                                    },
                                },
                            },
                            SchemaNode {
                                id: bethkit_schema::SchemaNodeId(4),
                                path: "TEST/items/element/value".to_owned(),
                                name: "Value".to_owned(),
                                required: true,
                                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Primitive {
                                    primitive: PrimitiveType::Integer {
                                        integer: IntegerType {
                                            width: 2,
                                            signed: false,
                                            byte_order,
                                        },
                                    },
                                },
                            },
                        ],
                    },
                }),
                count: ArrayCount::Prefixed {
                    integer: IntegerType {
                        width: 1,
                        signed: false,
                        byte_order,
                    },
                    terminator: None,
                },
            },
        };

        assert_eq!(
            node_data_size(&node, b"\x02\x03abc\x01\0\x02de\x02\0tail", false)?,
            12
        );
        Ok(())
    }

    /// Stops sizing a packed struct when its optional trailing suffix is absent.
    #[test]
    fn optional_struct_size_accepts_truncated_suffix(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let integer = IntegerType {
            width: 1,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
        };
        let fields = (0..3)
            .map(|index| SchemaNode {
                id: bethkit_schema::SchemaNodeId(index + 1),
                path: format!("TEST/value/{index}"),
                name: format!("Field {index}"),
                required: true,
                conflict_priority: bethkit_schema::ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::Integer { integer },
                },
            })
            .collect();
        let node = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST/value".to_owned(),
            name: "Value".to_owned(),
            required: true,
            conflict_priority: bethkit_schema::ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::OptionalStruct {
                fields,
                optional_from: 1,
            },
        };

        // when
        let required_only = node_data_size(&node, &[7], false)?;
        let one_optional = node_data_size(&node, &[7, 8], false)?;

        // then
        assert_eq!(required_only, 1);
        assert_eq!(one_optional, 2);
        Ok(())
    }

    #[test]
    fn windows_1252_strings_decode_without_losing_non_ascii_bytes() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: false,
            zero_terminated: true,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: None,
            allowed_values: Vec::new(),
        };

        let value = decode_string(&string, b"Gr\xfc\xdfe\0ignored", false, "TEST")
            .expect("Windows-1252 string should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "Grüße"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn windows_1252_strings_preserve_control_bytes() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: false,
            zero_terminated: false,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: None,
            allowed_values: Vec::new(),
        };

        let value =
            decode_string(&string, b"\x81", false, "TEST").expect("control byte should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "\u{81}"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn length_prefixed_strings_decode_padding_and_structural_terminator() {
        let string = StringType {
            encoding: "utf8".to_owned(),
            localized: false,
            zero_terminated: false,
            fixed_length: None,
            length_prefix: Some(bethkit_schema::StringLengthPrefix {
                width: 1,
                offset: 2,
            }),
            trailing_terminator: Some(b'|'),
            allowed_values: Vec::new(),
        };

        let value = decode_string(&string, b"\x03\0abc|", false, "TEST")
            .expect("length-prefixed string should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "abc"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn localized_strings_decode_exact_table_ids() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: true,
            zero_terminated: true,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: Some(b'|'),
            allowed_values: Vec::new(),
        };

        let bytes = 0x1234_5678_u32.to_le_bytes();
        let value = decode_string(&string, &bytes, true, "TEST")
            .expect("localized string ID should decode");

        match value {
            FieldValue::UInt(value) => assert_eq!(value, 0x1234_5678),
            other => panic!("expected string-table ID, got {other:?}"),
        }
    }
}
