// SPDX-License-Identifier: Apache-2.0
//!
//! Abstract syntax tree for Papyrus scripts.
//!
//! Declarations are plain nested structs, while expressions and statements
//! live in flat arenas inside [`Ast`] and refer to each other through
//! [`ExprId`] and [`StmtId`]. This keeps deep expressions free of per-node heap
//! allocations and makes whole-tree passes cache friendly.
//!
//! Every node carries a [`Span`]. Names are [`Symbol`]s from the script's
//! [`super::Interner`]; the original spelling of any node is available by
//! slicing the source with its span.

use std::ops::Index;

use super::interner::Symbol;
use super::token::Keyword;
use crate::diagnostics::Span;

/// Index of an [`Expr`] inside an [`Ast`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(u32);

/// Index of a [`Stmt`] inside an [`Ast`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StmtId(u32);

impl ExprId {
    /// Returns the position of the expression in [`Ast::exprs`].
    pub fn index(self) -> usize {
        // NOTE: ids are created from Vec lengths, so this widening never truncates.
        self.0 as usize
    }
}

impl StmtId {
    /// Returns the position of the statement in [`Ast::stmts`].
    pub fn index(self) -> usize {
        // NOTE: ids are created from Vec lengths, so this widening never truncates.
        self.0 as usize
    }
}

/// Arena holding all expressions and statements of one script.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    exprs: Vec<Expr>,
    stmts: Vec<Stmt>,
}

impl Ast {
    /// Returns all expressions in creation order (children before parents).
    pub fn exprs(&self) -> &[Expr] {
        &self.exprs
    }

    /// Returns all statements in creation order (children before parents).
    pub fn stmts(&self) -> &[Stmt] {
        &self.stmts
    }

    /// Returns the expression `id`, or `None` for an id from another tree.
    pub fn expr(&self, id: ExprId) -> Option<&Expr> {
        self.exprs.get(id.index())
    }

    /// Returns the statement `id`, or `None` for an id from another tree.
    pub fn stmt(&self, id: StmtId) -> Option<&Stmt> {
        self.stmts.get(id.index())
    }

    /// Adds an expression and returns its id.
    ///
    /// # Panics
    ///
    /// Panics after `u32::MAX` expressions, which cannot happen for sources
    /// limited to `u32::MAX` bytes because every expression consumes a token.
    pub(crate) fn push_expr(&mut self, kind: ExprKind, span: Span) -> ExprId {
        let id = ExprId(u32::try_from(self.exprs.len()).expect("fewer expressions than bytes"));
        self.exprs.push(Expr { kind, span });
        id
    }

    /// Adds a statement and returns its id.
    ///
    /// # Panics
    ///
    /// Panics after `u32::MAX` statements, which cannot happen for sources
    /// limited to `u32::MAX` bytes because every statement consumes a token.
    pub(crate) fn push_stmt(&mut self, kind: StmtKind, span: Span) -> StmtId {
        let id = StmtId(u32::try_from(self.stmts.len()).expect("fewer statements than bytes"));
        self.stmts.push(Stmt { kind, span });
        id
    }
}

impl Index<ExprId> for Ast {
    type Output = Expr;

    /// # Panics
    ///
    /// Panics if `id` belongs to another tree; use [`Ast::expr`] otherwise.
    fn index(&self, id: ExprId) -> &Expr {
        &self.exprs[id.index()]
    }
}

impl Index<StmtId> for Ast {
    type Output = Stmt;

    /// # Panics
    ///
    /// Panics if `id` belongs to another tree; use [`Ast::stmt`] otherwise.
    fn index(&self, id: StmtId) -> &Stmt {
        &self.stmts[id.index()]
    }
}

/// A name with its location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    /// The interned, case-insensitive name.
    pub symbol: Symbol,
    /// Where the name is written.
    pub span: Span,
}

/// A possibly namespaced name such as `Actor` or `DLC01:WorkshopScript`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QualifiedName {
    /// The `:`-separated parts, outermost namespace first; never empty.
    pub segments: Vec<Ident>,
    /// The location of the whole name.
    pub span: Span,
}

impl QualifiedName {
    /// Returns the last segment, the unqualified name.
    pub fn last(&self) -> Option<Ident> {
        self.segments.last().copied()
    }
}

/// The base of a type reference.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeName {
    /// `Bool`
    Bool,
    /// `Int`
    Int,
    /// `Float`
    Float,
    /// `String`
    String,
    /// `Var` (Fallout 4 and later).
    Var,
    /// A script or struct type.
    Named(QualifiedName),
}

/// A type as written in source, for example `Actor[]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeRef {
    /// The element type.
    pub name: TypeName,
    /// Whether the type is an array of `name`.
    pub is_array: bool,
    /// Where the type is written.
    pub span: Span,
}

/// A flag in a declaration header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlagName {
    /// A language flag such as `Global`, `Native`, `Auto` or `Const`.
    Keyword(Keyword),
    /// A user flag defined by a `.flg` file, such as `Hidden`.
    User(Symbol),
}

/// A flag with its location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Flag {
    /// Which flag this is.
    pub name: FlagName,
    /// Where the flag is written.
    pub span: Span,
}

/// A `{ ... }` documentation comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocComment {
    /// Location including the braces.
    pub span: Span,
}

impl DocComment {
    /// Returns the location of the text between the braces.
    pub fn text_span(self) -> Span {
        let start: u32 = self.span.start().saturating_add(1).min(self.span.end());
        let end: u32 = if self.span.len() >= 2 {
            self.span.end() - 1
        } else {
            self.span.end()
        };
        Span::new(start, end)
    }
}

/// A complete script file.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    /// The `ScriptName` header, if present.
    pub header: Option<ScriptHeader>,
    /// All declarations after the header, in source order.
    pub items: Vec<Item>,
}

/// `ScriptName Name [Extends Parent] [flags]`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptHeader {
    /// The script's name.
    pub name: QualifiedName,
    /// The parent script, if any.
    pub extends: Option<QualifiedName>,
    /// Header flags such as `Hidden`, `Conditional`, `Native` or `Const`.
    pub flags: Vec<Flag>,
    /// Documentation following the header.
    pub doc: Option<DocComment>,
    /// Location of the header line.
    pub span: Span,
}

/// A top-level or state-level declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// `Import Name`
    Import(Import),
    /// A script variable.
    Variable(Variable),
    /// A property outside a group.
    Property(Property),
    /// A property group (Fallout 4 and later).
    Group(Group),
    /// A struct definition (Fallout 4 and later).
    Struct(Struct),
    /// A custom event declaration (Fallout 4 and later).
    CustomEvent(CustomEvent),
    /// A guard declaration (Starfield).
    Guard(Guard),
    /// A named state.
    State(State),
    /// A function or event in the empty state.
    Function(Function),
}

/// `Import Name`
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    /// The imported script.
    pub name: QualifiedName,
    /// Location of the declaration.
    pub span: Span,
}

/// A script variable or struct member: `Type name [= value] [flags]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    /// The declared type.
    pub ty: TypeRef,
    /// The variable name.
    pub name: Ident,
    /// The initial value, which must be a constant.
    pub initial: Option<ExprId>,
    /// Flags such as `Conditional` or `Const`.
    pub flags: Vec<Flag>,
    /// Documentation following the declaration.
    pub doc: Option<DocComment>,
    /// Location of the declaration.
    pub span: Span,
}

/// How a property stores its value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyKind {
    /// `Auto`: backed by a hidden variable.
    Auto,
    /// `AutoReadOnly`: a constant getter.
    AutoReadOnly,
    /// A full property with explicit accessors.
    Full {
        /// The `Get` function, if defined.
        getter: Option<Box<Function>>,
        /// The `Set` function, if defined.
        setter: Option<Box<Function>>,
    },
}

/// `Type Property Name [= value] [flags]`, with accessors for full properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// The property type.
    pub ty: TypeRef,
    /// The property name.
    pub name: Ident,
    /// The initial value of an auto property.
    pub initial: Option<ExprId>,
    /// How the value is stored.
    pub kind: PropertyKind,
    /// Flags other than `Auto` and `AutoReadOnly`.
    pub flags: Vec<Flag>,
    /// Documentation following the declaration.
    pub doc: Option<DocComment>,
    /// Location of the declaration including accessors.
    pub span: Span,
}

/// `Group Name [flags] ... EndGroup`
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The group name.
    pub name: Ident,
    /// Group flags such as `CollapsedOnRef`.
    pub flags: Vec<Flag>,
    /// Documentation following the header.
    pub doc: Option<DocComment>,
    /// The properties in the group.
    pub properties: Vec<Property>,
    /// Location of the whole group.
    pub span: Span,
}

/// `Struct Name ... EndStruct`
#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    /// The struct name.
    pub name: Ident,
    /// Documentation following the header.
    pub doc: Option<DocComment>,
    /// The members in declaration order.
    pub members: Vec<Variable>,
    /// Location of the whole struct.
    pub span: Span,
}

/// `CustomEvent Name`
#[derive(Clone, Debug, PartialEq)]
pub struct CustomEvent {
    /// The event name.
    pub name: Ident,
    /// Location of the declaration.
    pub span: Span,
}

/// `Guard Name [flags]`
#[derive(Clone, Debug, PartialEq)]
pub struct Guard {
    /// The guard name.
    pub name: Ident,
    /// Guard flags such as `ProtectsFunctionLogic`.
    pub flags: Vec<Flag>,
    /// Documentation following the declaration.
    pub doc: Option<DocComment>,
    /// Location of the declaration.
    pub span: Span,
}

/// `[Auto] State Name ... EndState`
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// The state name.
    pub name: Ident,
    /// Whether this is the initial (`Auto`) state.
    pub is_auto: bool,
    /// The functions and events defined in the state.
    pub functions: Vec<Function>,
    /// Location of the whole state.
    pub span: Span,
}

/// Whether a [`Function`] is a function or an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionKind {
    /// `Function ... EndFunction`
    Function,
    /// `Event ... EndEvent`
    Event,
}

/// A function, event or property accessor.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// Function or event.
    pub kind: FunctionKind,
    /// The declared return type; `None` means no return value.
    pub return_type: Option<TypeRef>,
    /// The sender script of a remote event (`Event Actor.OnDeath(...)`).
    pub receiver: Option<QualifiedName>,
    /// The function name.
    pub name: Ident,
    /// The parameters in order.
    pub params: Vec<Parameter>,
    /// Flags such as `Global`, `Native`, `DebugOnly`.
    pub flags: Vec<Flag>,
    /// Documentation following the header.
    pub doc: Option<DocComment>,
    /// The body; `None` for native functions.
    pub body: Option<Vec<StmtId>>,
    /// Location of the whole function.
    pub span: Span,
}

/// `Type name [= default]`
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// The parameter type.
    pub ty: TypeRef,
    /// The parameter name.
    pub name: Ident,
    /// The default value, which must be a constant.
    pub default: Option<ExprId>,
    /// Location of the parameter.
    pub span: Span,
}

/// A statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    /// What the statement does.
    pub kind: StmtKind,
    /// Where the statement is written.
    pub span: Span,
}

/// The operator of an assignment statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    Add,
    /// `-=`
    Sub,
    /// `*=`
    Mul,
    /// `/=`
    Div,
    /// `%=`
    Mod,
}

/// A condition with the statements it guards.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalBlock {
    /// The condition.
    pub condition: ExprId,
    /// The statements run when the condition holds.
    pub body: Vec<StmtId>,
    /// Location from the keyword to the end of the body.
    pub span: Span,
}

/// The kinds of statements.
#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    /// `Type name [= value]`
    Define {
        /// The declared type.
        ty: TypeRef,
        /// The local variable name.
        name: Ident,
        /// The initial value.
        initial: Option<ExprId>,
        /// Whether the local is declared `Const` (Fallout 4 and later).
        is_const: bool,
    },
    /// `target op value`
    Assign {
        /// The assigned location: a name, member or array element.
        target: ExprId,
        /// The assignment operator.
        op: AssignOp,
        /// The assigned value.
        value: ExprId,
    },
    /// An expression evaluated for its side effects, usually a call.
    Expr(ExprId),
    /// `Return [value]`
    Return(Option<ExprId>),
    /// `If ... ElseIf ... Else ... EndIf`
    If {
        /// The `If` branch followed by all `ElseIf` branches.
        branches: Vec<ConditionalBlock>,
        /// The `Else` branch.
        otherwise: Option<Vec<StmtId>>,
    },
    /// `While condition ... EndWhile`
    While(ConditionalBlock),
    /// `LockGuard a, b ... EndLockGuard` (Starfield).
    LockGuard {
        /// The locked guards.
        guards: Vec<Ident>,
        /// The protected statements.
        body: Vec<StmtId>,
    },
    /// `TryLockGuard a, b ... [Else ...] EndTryLockGuard` (Starfield).
    TryLockGuard {
        /// The guards to lock.
        guards: Vec<Ident>,
        /// The statements run when all guards were locked.
        body: Vec<StmtId>,
        /// The statements run when a guard was already locked.
        otherwise: Option<Vec<StmtId>>,
    },
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// What the expression computes.
    pub kind: ExprKind,
    /// Where the expression is written.
    pub span: Span,
}

/// Prefix operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `!x`
    Not,
}

/// Infix operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `||`
    Or,
    /// `&&`
    And,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
}

/// A call argument, optionally named (`abForce = true`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Argument {
    /// The parameter name for a named argument.
    pub name: Option<Ident>,
    /// The argument value.
    pub value: ExprId,
}

/// The kinds of expressions.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// An integer literal; negative literals are folded.
    Int(i32),
    /// A float literal; negative literals are folded.
    Float(f32),
    /// `True` or `False`.
    Bool(bool),
    /// `None`.
    None,
    /// A string literal with escapes resolved, in the source's encoding.
    String(Box<[u8]>),
    /// A variable, property or type name.
    Name(Ident),
    /// A namespaced script name such as `DLC04:ManagerScript`, used as the
    /// receiver of a global function call (Fallout 4 and later).
    Path(QualifiedName),
    /// `Self`
    SelfRef,
    /// `Parent`
    Parent,
    /// A prefix operation.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: ExprId,
    },
    /// An infix operation.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        lhs: ExprId,
        /// The right operand.
        rhs: ExprId,
    },
    /// `value As Type`
    Cast {
        /// The converted value.
        value: ExprId,
        /// The target type.
        ty: TypeRef,
    },
    /// `value Is Type` (Fallout 4 and later).
    Is {
        /// The tested value.
        value: ExprId,
        /// The tested type.
        ty: TypeRef,
    },
    /// `object.name`
    Member {
        /// The accessed object.
        object: ExprId,
        /// The member name, including `Length` of arrays.
        name: Ident,
    },
    /// `array[index]`
    Index {
        /// The indexed array.
        array: ExprId,
        /// The element index.
        index: ExprId,
    },
    /// `[receiver.]name(args)`
    Call {
        /// The object the function is called on, if written.
        receiver: Option<ExprId>,
        /// The called function.
        name: Ident,
        /// The arguments in order.
        args: Vec<Argument>,
    },
    /// `New Type[size]`
    NewArray {
        /// The element type.
        element: TypeRef,
        /// The array length.
        size: ExprId,
    },
    /// `New StructType` (Fallout 4 and later).
    NewStruct {
        /// The created struct type.
        ty: TypeRef,
    },
    /// Placeholder for an expression that failed to parse. Only present when
    /// the parse produced error diagnostics.
    Error,
}
