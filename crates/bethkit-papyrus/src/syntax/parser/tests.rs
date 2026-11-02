// SPDX-License-Identifier: Apache-2.0
//!
//! Parser tests for all dialects, error recovery and robustness.

use crate::diagnostics::DiagnosticCode;
use crate::syntax::ast::{
    ExprId, ExprKind, FlagName, Function, FunctionKind, Item, PropertyKind, StmtKind, TypeName,
};
use crate::syntax::{parse, Dialect, Keyword, ParsedScript};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn parse_ok(source: &str, dialect: Dialect) -> Result<ParsedScript, Box<dyn std::error::Error>> {
    let parsed: ParsedScript = parse(source.as_bytes(), dialect)?;
    let messages: Vec<String> = parsed
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(messages.is_empty(), "unexpected diagnostics: {messages:#?}");
    Ok(parsed)
}

fn codes(parsed: &ParsedScript) -> Vec<DiagnosticCode> {
    parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn functions(parsed: &ParsedScript) -> Vec<&Function> {
    parsed
        .script()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(function) => Some(function),
            _ => None,
        })
        .collect()
}

/// Renders an expression in fully parenthesised prefix form for precise
/// structure assertions.
fn render(parsed: &ParsedScript, id: ExprId) -> String {
    let ast = parsed.ast();
    match &ast[id].kind {
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Float(value) => format!("{value:?}"),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::None => "none".to_owned(),
        ExprKind::String(bytes) => format!("{:?}", String::from_utf8_lossy(bytes)),
        ExprKind::Name(ident) => parsed.name(*ident).to_owned(),
        ExprKind::Path(path) => path
            .segments
            .iter()
            .map(|segment| parsed.name(*segment))
            .collect::<Vec<&str>>()
            .join(":"),
        ExprKind::SelfRef => "self".to_owned(),
        ExprKind::Parent => "parent".to_owned(),
        ExprKind::Unary { op, operand } => format!("({op:?} {})", render(parsed, *operand)),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({op:?} {} {})", render(parsed, *lhs), render(parsed, *rhs))
        }
        ExprKind::Cast { value, .. } => format!("(as {})", render(parsed, *value)),
        ExprKind::Is { value, .. } => format!("(is {})", render(parsed, *value)),
        ExprKind::Member { object, name } => {
            format!("{}.{}", render(parsed, *object), parsed.name(*name))
        }
        ExprKind::Index { array, index } => {
            format!("{}[{}]", render(parsed, *array), render(parsed, *index))
        }
        ExprKind::Call {
            receiver,
            name,
            args,
        } => {
            let receiver: String = receiver.map_or(String::new(), |r| render(parsed, r) + ".");
            let args: Vec<String> = args
                .iter()
                .map(|arg| match arg.name {
                    Some(name) => format!("{}={}", parsed.name(name), render(parsed, arg.value)),
                    None => render(parsed, arg.value),
                })
                .collect();
            format!("{receiver}{}({})", parsed.name(*name), args.join(", "))
        }
        ExprKind::NewArray { size, .. } => format!("new[{}]", render(parsed, *size)),
        ExprKind::NewStruct { .. } => "new struct".to_owned(),
        ExprKind::Error => "<error>".to_owned(),
    }
}

/// Verifies a representative Skyrim script with every declaration kind.
#[test]
fn parses_skyrim_declarations() -> TestResult {
    // given
    let source: &str = "\u{feff}ScriptName MyQuest extends Quest Conditional Hidden\n\
        {The quest script}\n\
        Import Utility\n\
        Int counter = -5 Conditional\n\
        Actor Property PlayerRef Auto\n\
        {The player}\n\
        Float Property Rate = 1.5 AutoReadOnly\n\
        Int[] Property Values Hidden\n\
        \tInt[] Function Get()\n\t\tReturn None\n\tEndFunction\n\
        \tFunction Set(Int[] aiValues)\n\tEndFunction\n\
        EndProperty\n\
        Int Function Add(Int a, Int b = 2) Global Native\n\
        Auto State Waiting\n\
        \tEvent OnUpdate()\n\tEndEvent\n\
        EndState\n";

    // when
    let parsed: ParsedScript = parse_ok(source, Dialect::Skyrim)?;

    // then
    let header = parsed.script().header.as_ref().ok_or("missing header")?;
    assert_eq!(parsed.name(header.name.last().ok_or("no name")?), "MyQuest");
    assert_eq!(header.flags.len(), 2);
    assert!(header.doc.is_some());

    let items = &parsed.script().items;
    assert_eq!(items.len(), 7);
    let Item::Variable(counter) = &items[1] else {
        return Err("expected a variable".into());
    };
    let initial: ExprId = counter.initial.ok_or("missing initial value")?;
    assert_eq!(render(&parsed, initial), "-5");

    let Item::Property(player) = &items[2] else {
        return Err("expected a property".into());
    };
    assert_eq!(player.kind, PropertyKind::Auto);
    assert!(player.flags.is_empty());
    assert!(player.doc.is_some());

    let Item::Property(values) = &items[4] else {
        return Err("expected a full property".into());
    };
    assert!(values.ty.is_array);
    let PropertyKind::Full { getter, setter } = &values.kind else {
        return Err("expected accessors".into());
    };
    assert!(getter.is_some() && setter.is_some());

    let add: &Function = functions(&parsed).first().copied().ok_or("missing Add")?;
    assert!(add.body.is_none());
    assert_eq!(add.params.len(), 2);
    assert!(add.params[1].default.is_some());
    assert!(add
        .flags
        .iter()
        .any(|flag| flag.name == FlagName::Keyword(Keyword::Global)));

    let Item::State(state) = &items[6] else {
        return Err("expected a state".into());
    };
    assert!(state.is_auto);
    assert_eq!(state.functions[0].kind, FunctionKind::Event);
    Ok(())
}

/// Verifies statements and the exact precedence of expressions.
#[test]
fn parses_statements_and_precedence() -> TestResult {
    // given
    let source: &str = "ScriptName S\n\
        Function Run(Actor akTarget)\n\
        \tInt i = 0\n\
        \tObjectReference[] refs = new ObjectReference[10]\n\
        \tWhile i < refs.Length && !akTarget.IsDead()\n\
        \t\ti += 1\n\
        \t\trefs[i] = akTarget.PlaceAtMe(Game.GetForm(0x14) As Form, aiCount = 1)\n\
        \tEndWhile\n\
        \tIf a || b && c == d + e * f\n\
        \tElseIf -x As Int > -2147483648\n\
        \t\tReturn\n\
        \tElse\n\
        \t\tDebug.Notification(\"Done: \\\"\" + i)\n\
        \tEndIf\n\
        \tparent.Run(self)\n\
        EndFunction\n";

    // when
    let parsed: ParsedScript = parse_ok(source, Dialect::Skyrim)?;

    // then
    let run: &Function = functions(&parsed)[0];
    let body: &Vec<_> = run.body.as_ref().ok_or("missing body")?;
    let ast = parsed.ast();
    let rendered: Vec<String> = body
        .iter()
        .map(|id| match &ast[*id].kind {
            StmtKind::Define {
                ty,
                initial: Some(initial),
                ..
            } => format!(
                "define {:?} {}",
                matches!(ty.name, TypeName::Int),
                render(&parsed, *initial)
            ),
            StmtKind::While(block) => {
                let inner: Vec<String> = block
                    .body
                    .iter()
                    .map(|id| match &ast[*id].kind {
                        StmtKind::Assign { target, op, value } => format!(
                            "{} {op:?} {}",
                            render(&parsed, *target),
                            render(&parsed, *value)
                        ),
                        other => format!("{other:?}"),
                    })
                    .collect();
                format!(
                    "while {} {{{}}}",
                    render(&parsed, block.condition),
                    inner.join("; ")
                )
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                let conditions: Vec<String> = branches
                    .iter()
                    .map(|branch| render(&parsed, branch.condition))
                    .collect();
                format!("if {} else={}", conditions.join(" | "), otherwise.is_some())
            }
            StmtKind::Expr(expr) => render(&parsed, *expr),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        rendered,
        vec![
            "define true 0".to_owned(),
            "define false new[10]".to_owned(),
            "while (And (Lt i refs.Length) (Not akTarget.IsDead())) {i Add 1; refs[i] Assign \
             akTarget.PlaceAtMe((as Game.GetForm(20)), aiCount=1)}"
                .to_owned(),
            "if (Or a (And b (Eq c (Add d (Mul e f))))) | (Gt (Neg (as x)) -2147483648) else=true"
                .to_owned(),
            "parent.Run(self)".to_owned(),
        ]
    );
    let StmtKind::If { otherwise, .. } = &ast[body[3]].kind else {
        return Err("expected If".into());
    };
    let notification: String = match &ast[otherwise.as_ref().ok_or("no else")?[0]].kind {
        StmtKind::Expr(expr) => render(&parsed, *expr),
        other => format!("{other:?}"),
    };
    assert_eq!(notification, "Debug.Notification((Add \"Done: \\\"\" i))");
    Ok(())
}

/// Verifies the Fallout 4 language additions.
#[test]
fn parses_fallout4_additions() -> TestResult {
    // given
    let source: &str = "ScriptName DLC01:Workshop:Builder extends ObjectReference Const\n\
        CustomEvent OnBuilt\n\
        Struct Entry\n\tForm Item\n\tInt Count = 1\n\t{How many}\nEndStruct\n\
        Group Settings CollapsedOnRef\n\
        \tInt Property Limit = 5 Auto Const Mandatory\n\
        EndGroup\n\
        Event Actor.OnDeath(Actor akSender, Var[] akArgs)\n\
        \tEntry e = new Entry\n\
        \tIf akSender is Actor\n\t\tVar v = e.Count\n\tEndIf\n\
        \tInt count = GetCount() Const\n\
        \tDLC04:DLC04_Manager.GetScript().Attach(akSender As Actor)\n\
        EndEvent\n";

    // when
    let parsed: ParsedScript = parse_ok(source, Dialect::Fallout4)?;

    // then
    let header = parsed.script().header.as_ref().ok_or("missing header")?;
    assert_eq!(header.name.segments.len(), 3);
    let items = &parsed.script().items;
    assert!(matches!(items[0], Item::CustomEvent(_)));
    let Item::Struct(entry) = &items[1] else {
        return Err("expected struct".into());
    };
    assert_eq!(entry.members.len(), 2);
    assert!(entry.members[1].doc.is_some());
    let Item::Group(group) = &items[2] else {
        return Err("expected group".into());
    };
    assert_eq!(group.properties.len(), 1);
    assert_eq!(group.properties[0].flags.len(), 2);
    let death: &Function = functions(&parsed)[0];
    assert!(death.receiver.is_some());
    assert!(matches!(death.params[1].ty.name, TypeName::Var));
    let body = death.body.as_ref().ok_or("missing body")?;
    let ast = parsed.ast();
    assert!(matches!(
        ast[body[2]].kind,
        StmtKind::Define { is_const: true, .. }
    ));
    let StmtKind::Expr(call) = ast[body[3]].kind else {
        return Err("expected a call statement".into());
    };
    assert_eq!(
        render(&parsed, call),
        "DLC04:DLC04_Manager.GetScript().Attach((as akSender))"
    );
    Ok(())
}

/// Verifies Starfield guard declarations and statements.
#[test]
fn parses_starfield_guards() -> TestResult {
    // given
    let source: &str = "ScriptName Vault\n\
        Guard Lock ProtectsFunctionLogic\n\
        Function Use()\n\
        \tLockGuard Lock\n\t\tx = 1\n\tEndLockGuard\n\
        \tTryLockGuard Lock, Other\n\t\tx = 2\n\tElse\n\t\tx = 3\n\tEndTryLockGuard\n\
        EndFunction\n";

    // when
    let parsed: ParsedScript = parse_ok(source, Dialect::Starfield)?;

    // then
    assert!(matches!(parsed.script().items[0], Item::Guard(_)));
    let body = functions(&parsed)[0].body.clone().ok_or("missing body")?;
    let ast = parsed.ast();
    assert!(matches!(&ast[body[0]].kind, StmtKind::LockGuard { guards, .. } if guards.len() == 1));
    assert!(matches!(
        &ast[body[1]].kind,
        StmtKind::TryLockGuard { guards, otherwise: Some(_), .. } if guards.len() == 2
    ));
    Ok(())
}

/// Verifies that newer keywords stay identifiers in older dialects.
#[test]
fn later_keywords_are_identifiers_in_skyrim() -> TestResult {
    // when
    let parsed: ParsedScript = parse_ok(
        "ScriptName S\nInt struct = 1\nFunction F(Int group)\n\tInt var = group\nEndFunction\n",
        Dialect::Skyrim,
    )?;

    // then
    assert_eq!(parsed.script().items.len(), 2);
    Ok(())
}

/// Verifies that errors are reported at the right construct and parsing
/// continues afterwards.
#[test]
fn recovers_from_errors() -> TestResult {
    // given
    let source: &str = "Int x\n\
        ScriptName S\n\
        Function A()\n\
        \tIf x\n\
        \t\t5 = x\n\
        EndFunction\n\
        Function B()\n\
        \tWhile True\n\
        \tEndIf\n\
        EndFunction\n\
        Event Actor.OnHit()\n\
        EndEvent\n\
        Int big = 2147483648\n\
        Function C()\n\
        \tx = (1 +\n\
        \tReturn 1 2\n\
        EndFunction\n\
        Function Last()\n";

    // when
    let parsed: ParsedScript = parse(source.as_bytes(), Dialect::Skyrim)?;

    // then
    assert_eq!(
        codes(&parsed),
        vec![
            DiagnosticCode::MissingScriptName,
            DiagnosticCode::MisplacedScriptName,
            DiagnosticCode::MissingEnd,
            DiagnosticCode::InvalidAssignmentTarget,
            DiagnosticCode::MismatchedEnd,
            DiagnosticCode::UnsupportedSyntax,
            DiagnosticCode::IntegerOverflow,
            DiagnosticCode::UnexpectedToken,
            DiagnosticCode::UnexpectedToken,
            DiagnosticCode::MissingEnd,
        ]
    );
    let names: Vec<&str> = functions(&parsed)
        .iter()
        .map(|function| parsed.name(function.name))
        .collect();
    assert_eq!(names, vec!["A", "B", "OnHit", "C", "Last"]);
    Ok(())
}

/// Verifies that pathological nesting is rejected instead of overflowing the
/// stack.
#[test]
fn limits_nesting_depth() -> TestResult {
    // given
    let deep: String = format!(
        "ScriptName S\nInt x = {}1{}\nFunction F()\n{}{}EndFunction\n",
        "(".repeat(10_000),
        ")".repeat(10_000),
        "If x\n".repeat(1_000),
        "EndIf\n".repeat(1_000)
    );

    // when
    let parsed: ParsedScript = parse(deep.as_bytes(), Dialect::Skyrim)?;

    // then
    assert!(codes(&parsed).contains(&DiagnosticCode::NestingTooDeep));
    Ok(())
}

/// Verifies that every prefix and corruption of a script parses without
/// panicking or looping.
#[test]
fn never_panics_on_damaged_input() -> TestResult {
    // given
    let source: &[u8] = b"ScriptName S extends Quest\n\
        Int[] Property P Auto\n\
        Struct T\n\tInt a\nEndStruct\n\
        State Busy\nEvent OnInit()\n\tIf (a.b[1] As T).c != \"s\\n\" || !x\n\
        \t\tf(a = 1, -2.5)\n\tElse\n\t\tReturn new Int[3]\n\tEndIf\nEndEvent\nEndState\n";
    let mut seed: u32 = 0x9E37_79B9;

    // when / then
    for dialect in [Dialect::Skyrim, Dialect::Fallout4, Dialect::Starfield] {
        for end in 0..=source.len() {
            parse(&source[..end], dialect)?;
        }
        for _ in 0..2_000 {
            let mut damaged: Vec<u8> = source.to_vec();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let index: usize = seed as usize % damaged.len();
                damaged[index] = (seed >> 24) as u8;
            }
            parse(&damaged, dialect)?;
        }
    }
    Ok(())
}
