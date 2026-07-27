// SPDX-License-Identifier: Apache-2.0
//!
//! Bounded, non-Turing-complete expressions used by schema conditions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Result, SchemaError, SchemaSignature};

/// Runtime inputs available to a schema expression.
pub struct EvalContext<'a> {
    /// Raw bytes of the payload currently being evaluated.
    pub payload: &'a [u8],
    /// Integer values decoded earlier in the same ordered payload grammar.
    pub field_values: &'a BTreeMap<String, i64>,
    /// Record form version from the plugin record header.
    pub form_version: u16,
    /// Signature of the containing record.
    pub record_signature: SchemaSignature,
}

/// A typed result produced by a schema expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalValue {
    /// Boolean condition result.
    Bool(bool),
    /// Signed integer result.
    Int(i64),
}

impl EvalValue {
    fn as_bool(self) -> Result<bool> {
        match self {
            Self::Bool(value) => Ok(value),
            Self::Int(_) => Err(SchemaError::Expression(
                "expected boolean expression result".to_owned(),
            )),
        }
    }

    fn as_int(self) -> Result<i64> {
        match self {
            Self::Int(value) => Ok(value),
            Self::Bool(_) => Err(SchemaError::Expression(
                "expected integer expression result".to_owned(),
            )),
        }
    }
}

/// Declarative condition expression supported by schema packages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expression {
    /// A boolean literal.
    Bool {
        /// Literal value.
        value: bool,
    },
    /// A signed integer literal.
    Int {
        /// Literal value.
        value: i64,
    },
    /// Length of the current payload in bytes.
    PayloadLength,
    /// Form version of the current record.
    FormVersion,
    /// Reads an unsigned little-endian integer from the payload.
    ReadUnsigned {
        /// Byte offset inside the current payload.
        offset: u32,
        /// Integer width in bytes. Supported values are 1, 2, 4, and 8.
        width: u8,
    },
    /// Reads an integer decoded earlier at one stable schema path.
    ReadField {
        /// Stable schema path of the earlier integer field.
        path: String,
    },
    /// Tests whether the containing record has a signature.
    RecordSignature {
        /// Signature to compare with the containing record.
        signature: SchemaSignature,
    },
    /// Equality comparison.
    Equal {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Inequality comparison.
    NotEqual {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Signed less-than comparison.
    LessThan {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Tests whether a bit is set in an integer expression.
    BitSet {
        /// Integer operand.
        value: Box<Expression>,
        /// Zero-based bit index.
        bit: u8,
    },
    /// Boolean conjunction.
    And {
        /// Operands evaluated from left to right.
        values: Vec<Expression>,
    },
    /// Boolean disjunction.
    Or {
        /// Operands evaluated from left to right.
        values: Vec<Expression>,
    },
    /// Boolean negation.
    Not {
        /// Operand to negate.
        value: Box<Expression>,
    },
    /// Checked signed addition.
    Add {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Checked signed subtraction.
    Subtract {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Checked signed multiplication.
    Multiply {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Checked signed integer division.
    Divide {
        /// Dividend.
        left: Box<Expression>,
        /// Non-zero divisor.
        right: Box<Expression>,
    },
    /// Number of set bits in the 64-bit representation of an integer.
    BitCount {
        /// Integer operand.
        value: Box<Expression>,
    },
    /// Selects one of two equally typed values from a boolean condition.
    Select {
        /// Boolean condition.
        condition: Box<Expression>,
        /// Result when the condition is true.
        if_true: Box<Expression>,
        /// Result when the condition is false.
        if_false: Box<Expression>,
    },
}

impl Expression {
    /// Evaluates the expression with a bounded operation budget.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::Expression`] for type mismatches, invalid reads,
    /// arithmetic overflow, or exhausted operation budgets.
    pub fn evaluate(&self, context: &EvalContext<'_>, max_operations: u32) -> Result<EvalValue> {
        let mut remaining: u32 = max_operations;
        self.evaluate_inner(context, &mut remaining)
    }

    fn evaluate_inner(&self, context: &EvalContext<'_>, remaining: &mut u32) -> Result<EvalValue> {
        if *remaining == 0 {
            return Err(SchemaError::Expression(
                "operation budget exhausted".to_owned(),
            ));
        }
        *remaining -= 1;

        match self {
            Self::Bool { value } => Ok(EvalValue::Bool(*value)),
            Self::Int { value } => Ok(EvalValue::Int(*value)),
            Self::PayloadLength => {
                let length: i64 = i64::try_from(context.payload.len()).map_err(|_| {
                    SchemaError::Expression("payload length exceeds i64".to_owned())
                })?;
                Ok(EvalValue::Int(length))
            }
            Self::FormVersion => Ok(EvalValue::Int(i64::from(context.form_version))),
            Self::ReadUnsigned { offset, width } => {
                read_unsigned(context.payload, *offset, *width).map(EvalValue::Int)
            }
            Self::ReadField { path } => context
                .field_values
                .get(path)
                .copied()
                .map(EvalValue::Int)
                .ok_or_else(|| {
                    SchemaError::Expression(format!(
                        "field {path} is unavailable at this grammar position"
                    ))
                }),
            Self::RecordSignature { signature } => {
                Ok(EvalValue::Bool(*signature == context.record_signature))
            }
            Self::Equal { left, right } => {
                let left_value: EvalValue = left.evaluate_inner(context, remaining)?;
                let right_value: EvalValue = right.evaluate_inner(context, remaining)?;
                Ok(EvalValue::Bool(left_value == right_value))
            }
            Self::NotEqual { left, right } => {
                let left_value: EvalValue = left.evaluate_inner(context, remaining)?;
                let right_value: EvalValue = right.evaluate_inner(context, remaining)?;
                Ok(EvalValue::Bool(left_value != right_value))
            }
            Self::LessThan { left, right } => {
                let left_value: i64 = left.evaluate_inner(context, remaining)?.as_int()?;
                let right_value: i64 = right.evaluate_inner(context, remaining)?.as_int()?;
                Ok(EvalValue::Bool(left_value < right_value))
            }
            Self::BitSet { value, bit } => {
                if *bit >= 64 {
                    return Err(SchemaError::Expression(
                        "bit index must be below 64".to_owned(),
                    ));
                }
                let raw: i64 = value.evaluate_inner(context, remaining)?.as_int()?;
                Ok(EvalValue::Bool((raw as u64) & (1_u64 << bit) != 0))
            }
            Self::And { values } => {
                for value in values {
                    if !value.evaluate_inner(context, remaining)?.as_bool()? {
                        return Ok(EvalValue::Bool(false));
                    }
                }
                Ok(EvalValue::Bool(true))
            }
            Self::Or { values } => {
                for value in values {
                    if value.evaluate_inner(context, remaining)?.as_bool()? {
                        return Ok(EvalValue::Bool(true));
                    }
                }
                Ok(EvalValue::Bool(false))
            }
            Self::Not { value } => {
                let result: bool = value.evaluate_inner(context, remaining)?.as_bool()?;
                Ok(EvalValue::Bool(!result))
            }
            Self::Add { left, right } => {
                let left_value: i64 = left.evaluate_inner(context, remaining)?.as_int()?;
                let right_value: i64 = right.evaluate_inner(context, remaining)?.as_int()?;
                left_value
                    .checked_add(right_value)
                    .map(EvalValue::Int)
                    .ok_or_else(|| SchemaError::Expression("signed addition overflowed".to_owned()))
            }
            Self::Subtract { left, right } => {
                let left_value: i64 = left.evaluate_inner(context, remaining)?.as_int()?;
                let right_value: i64 = right.evaluate_inner(context, remaining)?.as_int()?;
                left_value
                    .checked_sub(right_value)
                    .map(EvalValue::Int)
                    .ok_or_else(|| {
                        SchemaError::Expression("signed subtraction overflowed".to_owned())
                    })
            }
            Self::Multiply { left, right } => {
                let left_value: i64 = left.evaluate_inner(context, remaining)?.as_int()?;
                let right_value: i64 = right.evaluate_inner(context, remaining)?.as_int()?;
                left_value
                    .checked_mul(right_value)
                    .map(EvalValue::Int)
                    .ok_or_else(|| {
                        SchemaError::Expression("signed multiplication overflowed".to_owned())
                    })
            }
            Self::Divide { left, right } => {
                let left_value: i64 = left.evaluate_inner(context, remaining)?.as_int()?;
                let right_value: i64 = right.evaluate_inner(context, remaining)?.as_int()?;
                left_value
                    .checked_div(right_value)
                    .map(EvalValue::Int)
                    .ok_or_else(|| {
                        SchemaError::Expression("signed division by zero or overflowed".to_owned())
                    })
            }
            Self::BitCount { value } => {
                let raw: i64 = value.evaluate_inner(context, remaining)?.as_int()?;
                Ok(EvalValue::Int(i64::from((raw as u64).count_ones())))
            }
            Self::Select {
                condition,
                if_true,
                if_false,
            } => {
                if condition.evaluate_inner(context, remaining)?.as_bool()? {
                    if_true.evaluate_inner(context, remaining)
                } else {
                    if_false.evaluate_inner(context, remaining)
                }
            }
        }
    }
}

fn read_unsigned(payload: &[u8], offset: u32, width: u8) -> Result<i64> {
    let start: usize = offset as usize;
    let end: usize = start
        .checked_add(width as usize)
        .ok_or_else(|| SchemaError::Expression("read range overflowed".to_owned()))?;
    let bytes: &[u8] = payload.get(start..end).ok_or_else(|| {
        SchemaError::Expression(format!(
            "read range {start}..{end} exceeds payload length {}",
            payload.len()
        ))
    })?;

    let value: u64 = match width {
        1 => u64::from(bytes[0]),
        2 => u64::from(u16::from_le_bytes([bytes[0], bytes[1]])),
        4 => u64::from(u32::from_le_bytes(
            bytes.try_into().expect("slice length was checked"),
        )),
        8 => u64::from_le_bytes(bytes.try_into().expect("slice length was checked")),
        _ => {
            return Err(SchemaError::Expression(format!(
                "unsupported integer width {width}"
            )));
        }
    };
    i64::try_from(value).map_err(|_| {
        SchemaError::Expression("unsigned value cannot be represented as i64".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies payload reads and boolean conditions.
    #[test]
    fn expression_reads_payload_and_tests_bit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let expression = Expression::BitSet {
            value: Box::new(Expression::ReadUnsigned {
                offset: 1,
                width: 1,
            }),
            bit: 2,
        };
        let field_values = BTreeMap::new();
        let context = EvalContext {
            payload: &[0, 4],
            field_values: &field_values,
            form_version: 44,
            record_signature: SchemaSignature(*b"TEST"),
        };

        // when
        let result = expression.evaluate(&context, 16)?;

        // then
        assert_eq!(result, EvalValue::Bool(true));
        Ok(())
    }

    /// Verifies that expression budgets stop excessively deep evaluation.
    #[test]
    fn expression_budget_is_enforced() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let expression = Expression::Not {
            value: Box::new(Expression::Bool { value: true }),
        };
        let field_values = BTreeMap::new();
        let context = EvalContext {
            payload: &[],
            field_values: &field_values,
            form_version: 0,
            record_signature: SchemaSignature(*b"TEST"),
        };

        // when
        let result = expression.evaluate(&context, 1);

        // then
        assert!(matches!(result, Err(SchemaError::Expression(_))));
        Ok(())
    }

    /// Verifies bounded conditional selection used by xEdit union deciders.
    #[test]
    fn expression_selects_union_variant() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let expression = Expression::Select {
            condition: Box::new(Expression::LessThan {
                left: Box::new(Expression::FormVersion),
                right: Box::new(Expression::Int { value: 44 }),
            }),
            if_true: Box::new(Expression::Int { value: 0 }),
            if_false: Box::new(Expression::Int { value: 1 }),
        };
        let field_values = BTreeMap::new();
        let context = EvalContext {
            payload: &[],
            field_values: &field_values,
            form_version: 44,
            record_signature: SchemaSignature(*b"TEST"),
        };

        // when
        let result = expression.evaluate(&context, 16)?;

        // then
        assert_eq!(result, EvalValue::Int(1));
        Ok(())
    }

    /// Resolves an earlier field and applies bounded count arithmetic.
    #[test]
    fn expression_reads_fields_and_computes_counts(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let path = "TEST/payload/Flags".to_owned();
        let expression = Expression::Multiply {
            left: Box::new(Expression::BitCount {
                value: Box::new(Expression::ReadField { path: path.clone() }),
            }),
            right: Box::new(Expression::Int { value: 2 }),
        };
        let field_values = BTreeMap::from([(path, 0b1011)]);
        let context = EvalContext {
            payload: &[],
            field_values: &field_values,
            form_version: 0,
            record_signature: SchemaSignature(*b"TEST"),
        };

        assert_eq!(expression.evaluate(&context, 16)?, EvalValue::Int(6));
        Ok(())
    }
}
