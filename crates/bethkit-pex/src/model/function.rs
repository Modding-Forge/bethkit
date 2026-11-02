// SPDX-License-Identifier: Apache-2.0
//!
//! Functions and their instructions.
//!
//! All arguments of a function live in one vector; each [`Instruction`] only
//! records where its arguments start and how many there are. Scripts contain
//! millions of instructions, so this avoids one allocation per instruction.

use super::{FunctionFlags, StringId, Value};
use crate::error::{PexError, Result};
use crate::opcode::Opcode;

/// A name together with its type, used for parameters and locals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypedName {
    /// The name.
    pub name: StringId,
    /// The type name.
    pub type_name: StringId,
}

/// One instruction of a [`Function`]; its arguments are stored in the function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Instruction {
    opcode: Opcode,
    first_argument: u32,
    variadic_count: u16,
}

impl Instruction {
    /// Returns the opcode.
    pub fn opcode(self) -> Opcode {
        self.opcode
    }
}

/// An instruction with its arguments, borrowed from a [`Function`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstructionRef<'f> {
    /// The opcode.
    pub opcode: Opcode,
    /// The fixed arguments, as many as [`crate::OpcodeInfo::fixed_args`].
    pub fixed: &'f [Value],
    /// The counted trailing arguments of variadic opcodes.
    pub variadic: &'f [Value],
}

/// A function, event or property accessor.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// Return type; `None` for no return value.
    pub return_type: StringId,
    /// Documentation of the function.
    pub doc_string: StringId,
    /// User flags of the function.
    pub user_flags: u32,
    /// Function flags such as global and native.
    pub flags: FunctionFlags,
    /// Parameters in order.
    pub params: Vec<TypedName>,
    /// Local variables, including compiler temporaries.
    pub locals: Vec<TypedName>,
    instructions: Vec<Instruction>,
    arguments: Vec<Value>,
}

impl Function {
    /// Creates a function without instructions.
    pub fn new(
        return_type: StringId,
        doc_string: StringId,
        user_flags: u32,
        flags: FunctionFlags,
        params: Vec<TypedName>,
        locals: Vec<TypedName>,
    ) -> Self {
        Self {
            return_type,
            doc_string,
            user_flags,
            flags,
            params,
            locals,
            instructions: Vec::new(),
            arguments: Vec::new(),
        }
    }

    /// Reserves space for `instructions` instructions with `arguments`
    /// arguments in total.
    pub fn reserve(&mut self, instructions: usize, arguments: usize) {
        self.instructions.reserve(instructions);
        self.arguments.reserve(arguments);
    }

    /// Returns the number of instructions.
    pub fn instruction_count(&self) -> usize {
        self.instructions.len()
    }

    /// Returns the instruction at `index` with its arguments.
    pub fn instruction(&self, index: usize) -> Option<InstructionRef<'_>> {
        self.instructions
            .get(index)
            .map(|instruction| self.resolve(*instruction))
    }

    /// Iterates over all instructions with their arguments.
    pub fn instructions(&self) -> impl ExactSizeIterator<Item = InstructionRef<'_>> {
        self.instructions
            .iter()
            .map(|instruction| self.resolve(*instruction))
    }

    /// Returns all arguments of all instructions in order, for in-place edits
    /// such as replacing string IDs.
    pub fn arguments_mut(&mut self) -> &mut [Value] {
        &mut self.arguments
    }

    /// Appends an instruction.
    ///
    /// # Errors
    ///
    /// Returns [`PexError::InvalidArity`] if `fixed` does not match the
    /// opcode's fixed argument count, if a non-variadic opcode gets variadic
    /// arguments, or if there are more than 65535 of them; and
    /// [`PexError::TooManyEntries`] if the function's arguments exceed the
    /// 32-bit index range.
    pub fn push_instruction(
        &mut self,
        opcode: Opcode,
        fixed: &[Value],
        variadic: &[Value],
    ) -> Result<()> {
        let info = opcode.info();
        let variadic_count: u16 = u16::try_from(variadic.len())
            .ok()
            .filter(|_| fixed.len() == usize::from(info.fixed_args))
            .filter(|count| info.variadic || *count == 0)
            .ok_or(PexError::InvalidArity {
                mnemonic: info.mnemonic,
                fixed: fixed.len(),
                variadic: variadic.len(),
            })?;
        let first_argument: u32 =
            u32::try_from(self.arguments.len()).map_err(|_| PexError::TooManyEntries {
                what: "function arguments",
                len: self.arguments.len(),
            })?;
        self.arguments.extend_from_slice(fixed);
        self.arguments.extend_from_slice(variadic);
        self.instructions.push(Instruction {
            opcode,
            first_argument,
            variadic_count,
        });
        Ok(())
    }

    /// Appends an instruction whose arguments were already appended with
    /// `push_argument`, starting at `first_argument`.
    pub(crate) fn push_raw_instruction(
        &mut self,
        opcode: Opcode,
        first_argument: u32,
        variadic_count: u16,
    ) {
        self.instructions.push(Instruction {
            opcode,
            first_argument,
            variadic_count,
        });
    }

    /// Appends one argument of the instruction being read.
    pub(crate) fn push_argument(&mut self, value: Value) {
        self.arguments.push(value);
    }

    /// Returns the number of stored arguments.
    pub(crate) fn argument_count(&self) -> usize {
        self.arguments.len()
    }

    fn resolve(&self, instruction: Instruction) -> InstructionRef<'_> {
        let fixed_len: usize = usize::from(instruction.opcode.info().fixed_args);
        // NOTE: u32 always fits into usize on the targets bethkit supports.
        let start: usize = instruction.first_argument as usize;
        let variadic_end: usize = start + fixed_len + usize::from(instruction.variadic_count);
        // Instructions are only created with all their arguments in place, so the
        // ranges are always in bounds; `get` keeps a corrupted model from panicking.
        let fixed: &[Value] = self.arguments.get(start..start + fixed_len).unwrap_or(&[]);
        let variadic: &[Value] = self
            .arguments
            .get(start + fixed_len..variadic_end)
            .unwrap_or(&[]);
        InstructionRef {
            opcode: instruction.opcode,
            fixed,
            variadic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that instructions resolve to their own arguments.
    #[test]
    fn instructions_resolve_arguments() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut function = Function::new(
            StringId::new(0),
            StringId::new(0),
            0,
            FunctionFlags::empty(),
            Vec::new(),
            Vec::new(),
        );
        let name = Value::Identifier(StringId::new(1));
        let text = Value::String(StringId::new(2));

        // when
        function.push_instruction(Opcode::Assign, &[name, text], &[])?;
        function.push_instruction(Opcode::CallStatic, &[name, name, name], &[text, text])?;
        let wrong_fixed = function.push_instruction(Opcode::Assign, &[name], &[]);
        let wrong_variadic = function.push_instruction(Opcode::Assign, &[name, name], &[text]);

        // then
        let call: InstructionRef<'_> = function.instruction(1).ok_or("missing call")?;
        assert_eq!(call.fixed.len(), 3);
        assert_eq!(call.variadic, &[text, text]);
        assert_eq!(function.instructions().len(), 2);
        assert!(wrong_fixed.is_err());
        assert!(wrong_variadic.is_err());
        function.arguments_mut()[1] = Value::Integer(5);
        assert_eq!(
            function.instruction(0).map(|i| i.fixed[1]),
            Some(Value::Integer(5))
        );
        Ok(())
    }
}
