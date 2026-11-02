// SPDX-License-Identifier: Apache-2.0
//! Synthetic round-trip, error and robustness tests for every game layout.

use std::borrow::Cow;

use bethkit_pex::model::{
    DebugFunction, DebugFunctionKind, DebugInfo, Function, FunctionFlags, Header, NamedFunction,
    Object, Property, PropertyFlags, PropertyGroup, State, StringId, StringTable, Struct,
    StructMember, StructOrder, TypedName, UserFlag, Value, Variable,
};
use bethkit_pex::{Opcode, PexError, PexFile, PexGame, PexView};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const GAMES: [PexGame; 4] = [
    PexGame::Skyrim,
    PexGame::Fallout4,
    PexGame::Fallout76,
    PexGame::Starfield,
];

/// Builds a script that uses every structure the game's layout supports.
fn sample(game: PexGame) -> Result<PexFile<'static>, Box<dyn std::error::Error>> {
    let profile = game.profile();
    let mut strings = StringTable::new();
    let mut id = |text: &str| strings.push(text.as_bytes().to_vec());
    let empty = id("")?;
    let script = id("MyScript")?;
    let parent = id("Quest")?;
    let int = id("Int")?;
    let string_type = id("String")?;
    let none = id("None")?;
    let count = id("Count")?;
    let count_var = id("::Count_var")?;
    let temp = id("::temp0")?;
    let notify = id("Notification")?;
    let debug = id("Debug")?;
    let text = id("Hello \u{e4}")?;
    let on_init = id("OnInit")?;
    let hidden = id("hidden")?;
    let label = id("Label")?;
    let point = id("Point")?;
    let guard = id("Lock")?;
    let fallout4 = profile.has_fallout4_layout;

    let mut function = Function::new(
        none,
        empty,
        0,
        FunctionFlags::empty(),
        vec![TypedName {
            name: count,
            type_name: int,
        }],
        vec![TypedName {
            name: temp,
            type_name: string_type,
        }],
    );
    function.push_instruction(
        Opcode::Assign,
        &[Value::Identifier(temp), Value::String(text)],
        &[],
    )?;
    function.push_instruction(
        Opcode::CallStatic,
        &[
            Value::Identifier(debug),
            Value::Identifier(notify),
            Value::Identifier(temp),
        ],
        &[
            Value::Identifier(temp),
            Value::Float(f32::from_bits(0x7FA0_0001)),
        ],
    )?;
    function.push_instruction(
        Opcode::IAdd,
        &[Value::Identifier(count), Value::Integer(-7), Value::Bool(1)],
        &[],
    )?;
    if fallout4 {
        function.push_instruction(
            Opcode::Is,
            &[
                Value::Identifier(temp),
                Value::Identifier(temp),
                Value::Identifier(int),
            ],
            &[],
        )?;
    }
    if profile.has_guards {
        function.push_instruction(Opcode::LockGuards, &[], &[Value::Identifier(guard)])?;
    }
    function.push_instruction(Opcode::Return, &[Value::None], &[])?;
    let line_numbers: Vec<u16> = (1..=function.instruction_count() as u16).collect();

    let mut getter = Function::new(int, empty, 0, FunctionFlags::empty(), vec![], vec![]);
    getter.push_instruction(Opcode::Return, &[Value::Identifier(count_var)], &[])?;

    let object = Object {
        name: script,
        declared_size: None,
        parent_class_name: parent,
        doc_string: empty,
        const_flag: u8::from(fallout4),
        user_flags: 1,
        auto_state_name: empty,
        structs: if fallout4 {
            vec![Struct {
                name: point,
                members: vec![StructMember {
                    name: label,
                    type_name: string_type,
                    user_flags: 0,
                    initial_value: Value::String(text),
                    const_flag: 1,
                    doc_string: empty,
                }],
            }]
        } else {
            vec![]
        },
        variables: vec![Variable {
            name: count_var,
            type_name: int,
            user_flags: 0,
            initial_value: Value::Integer(3),
            const_flag: 0,
        }],
        guards: if profile.has_guards {
            vec![guard]
        } else {
            vec![]
        },
        properties: vec![
            Property {
                name: count,
                type_name: int,
                doc_string: empty,
                user_flags: 1,
                flags: PropertyFlags::all(),
                auto_var_name: Some(count_var),
                getter: None,
                setter: None,
            },
            Property {
                name: label,
                type_name: int,
                doc_string: empty,
                user_flags: 0,
                // An unknown bit must survive the round trip.
                flags: PropertyFlags::from_bits_retain(0x81),
                auto_var_name: None,
                getter: Some(getter),
                setter: None,
            },
        ],
        states: vec![State {
            name: empty,
            functions: vec![NamedFunction {
                name: on_init,
                function,
            }],
        }],
    };

    let debug_info = DebugInfo {
        modification_time: 1_700_000_000,
        functions: vec![DebugFunction {
            object_name: script,
            state_name: empty,
            function_name: on_init,
            kind: DebugFunctionKind::Other(9),
            line_numbers,
        }],
        property_groups: if fallout4 {
            vec![PropertyGroup {
                object_name: script,
                group_name: empty,
                doc_string: empty,
                user_flags: 0,
                property_names: vec![count, label],
            }]
        } else {
            vec![]
        },
        struct_orders: if fallout4 {
            vec![StructOrder {
                object_name: script,
                struct_name: point,
                member_names: vec![label],
            }]
        } else {
            vec![]
        },
    };

    Ok(PexFile {
        game,
        header: Header {
            major_version: profile.default_version.0,
            minor_version: profile.default_version.1,
            game_id: profile.game_id,
            compilation_time: 1_700_000_100,
            source_file_name: Cow::Borrowed(b"MyScript.psc"),
            user_name: Cow::Borrowed(b"modder"),
            computer_name: Cow::Borrowed(b"PC"),
        },
        strings,
        debug_info: Some(debug_info),
        user_flags: vec![UserFlag {
            name: hidden,
            bit: 0,
        }],
        objects: vec![object],
        trailing_data: Cow::Borrowed(&[]),
    })
}

/// Writes, reads and rewrites a script of every game and requires identity.
#[test]
fn every_game_round_trips_exactly() -> TestResult {
    for game in GAMES {
        // given
        let original: PexFile<'static> = sample(game)?;

        // when
        let bytes: Vec<u8> = original.write()?;
        let parsed: PexFile<'_> = PexFile::parse(&bytes)?;
        let rewritten: Vec<u8> = parsed.write()?;
        let viewed: PexFile<'_> = PexView::parse(&bytes)?.into_file()?;

        // then
        assert_eq!(parsed, original, "{game}");
        assert_eq!(rewritten, bytes, "{game}");
        assert_eq!(viewed, original, "{game}");
        let magic: [u8; 4] = bytes[..4].try_into()?;
        let expected: [u8; 4] = if game == PexGame::Skyrim {
            [0xFA, 0x57, 0xC0, 0xDE]
        } else {
            [0xDE, 0xC0, 0x57, 0xFA]
        };
        assert_eq!(magic, expected, "{game}");
        let float = parsed.objects[0].states[0].functions[0]
            .function
            .instruction(1)
            .ok_or("missing call")?
            .variadic[1];
        assert!(matches!(float, Value::Float(f) if f.to_bits() == 0x7FA0_0001));
    }
    Ok(())
}

/// Verifies that the view exposes the preamble and decodes objects lazily.
#[test]
fn view_decodes_objects_on_demand() -> TestResult {
    // given
    let bytes: Vec<u8> = sample(PexGame::Starfield)?.write()?;

    // when
    let view = PexView::parse(&bytes)?;

    // then
    assert_eq!(view.game(), PexGame::Starfield);
    assert_eq!(view.object_count(), 1);
    assert_eq!(view.strings().get(StringId::new(1)), Some(&b"MyScript"[..]));
    assert_eq!(view.object(0)?.guards.len(), 1);
    assert!(matches!(
        view.object(1),
        Err(PexError::ObjectOutOfRange { index: 1, count: 1 })
    ));
    Ok(())
}

/// Verifies that the writer refuses data the game cannot store.
#[test]
fn writer_rejects_unsupported_data() -> TestResult {
    // given
    let mut skyrim: PexFile<'static> = sample(PexGame::Fallout4)?;
    skyrim.game = PexGame::Skyrim;
    let mut fallout4: PexFile<'static> = sample(PexGame::Starfield)?;
    fallout4.game = PexGame::Fallout4;
    fallout4.objects[0].guards.clear();

    // then
    assert!(matches!(
        skyrim.write(),
        Err(PexError::UnsupportedFeature { .. })
    ));
    assert!(matches!(
        fallout4.write(),
        Err(PexError::OpcodeNotInGame { opcode: 0x30, .. })
    ));
    Ok(())
}

/// Verifies precise errors for malformed headers and instructions.
#[test]
fn malformed_files_report_errors() -> TestResult {
    // given
    let bytes: Vec<u8> = sample(PexGame::Skyrim)?.write()?;
    // ASSIGN, identifier #8 (`::temp0`, big-endian), then a string literal tag.
    let assign: usize = bytes
        .windows(5)
        .position(|window| window == [0x0D, 0x01, 0x00, 0x08, 0x02])
        .ok_or("ASSIGN not found")?;

    let mut wrong_magic = bytes.clone();
    wrong_magic[0] = 0;
    let mut wrong_version = bytes.clone();
    wrong_version[4] = 2;
    let mut fallout4_opcode = bytes.clone();
    fallout4_opcode[assign] = 0x24;
    let mut unknown_opcode = bytes.clone();
    unknown_opcode[assign] = 0x60;
    let mut bad_value = bytes.clone();
    bad_value[assign + 1] = 9;
    let mut unknown_game = sample(PexGame::Fallout4)?.write()?;
    unknown_game[6] = 9;

    // then
    assert!(matches!(
        PexFile::parse(&wrong_magic),
        Err(PexError::InvalidMagic { .. })
    ));
    assert!(matches!(
        PexFile::parse(&wrong_version),
        Err(PexError::UnsupportedVersion { major: 2, .. })
    ));
    assert!(matches!(
        PexFile::parse(&fallout4_opcode),
        Err(PexError::OpcodeNotInGame { opcode: 0x24, offset, .. }) if offset == assign
    ));
    assert!(matches!(
        PexFile::parse(&unknown_opcode),
        Err(PexError::InvalidOpcode { opcode: 0x60, .. })
    ));
    assert!(matches!(
        PexFile::parse(&bad_value),
        Err(PexError::InvalidValueType { tag: 9, .. })
    ));
    assert!(matches!(
        PexFile::parse(&unknown_game),
        Err(PexError::UnknownGame { game_id: 9 })
    ));
    Ok(())
}

/// Keeps bytes after the last object and wrong object sizes, as written by
/// some tools and third-party compilers, and reproduces them exactly.
#[test]
fn tolerated_irregularities_round_trip() -> TestResult {
    for game in GAMES {
        // given
        let bytes: Vec<u8> = sample(game)?.write()?;
        let mut trailing: Vec<u8> = bytes.clone();
        trailing.extend_from_slice(b"\0garbage");
        // The object size field follows the two-byte object name after the count.
        let size_offset: usize = bytes.len() - object_len(&bytes)? + 2;
        let mut wrong_size: Vec<u8> = bytes.clone();
        wrong_size[size_offset..size_offset + 4].copy_from_slice(&[0xA6, 0, 0, 0]);

        for (variant, data) in [("trailing", &trailing), ("wrong size", &wrong_size)] {
            // when
            let parsed: PexFile<'_> = PexFile::parse(data)?;
            let viewed: PexFile<'_> = PexView::parse(data)?.into_file()?;

            // then
            assert_eq!(&parsed.write()?, data, "{game} {variant}");
            assert_eq!(viewed, parsed, "{game} {variant}");
        }
        assert_eq!(&*PexFile::parse(&trailing)?.trailing_data, b"\0garbage");
        assert!(PexFile::parse(&wrong_size)?.objects[0]
            .declared_size
            .is_some());
        assert!(PexFile::parse(&bytes)?.objects[0].declared_size.is_none());
    }
    Ok(())
}

/// Returns the length of the single object of a sample file, including its
/// name, as the difference to the same file without objects.
fn object_len(bytes: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
    let mut without_object: PexFile<'_> = PexFile::parse(bytes)?;
    without_object.objects.clear();
    Ok(bytes.len() - without_object.write()?.len())
}

/// Parses every prefix and thousands of corrupted copies without panicking, and
/// requires every corrupted copy that still parses to round-trip exactly.
#[test]
fn damaged_input_never_panics() -> TestResult {
    for game in GAMES {
        // given
        let bytes: Vec<u8> = sample(game)?.write()?;
        let mut seed: u32 = 0x2545_F491;

        // when / then
        for end in 0..bytes.len() {
            assert!(PexFile::parse(&bytes[..end]).is_err());
            let _ = PexView::parse(&bytes[..end]);
        }
        for _ in 0..5_000 {
            let mut damaged: Vec<u8> = bytes.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let index: usize = seed as usize % damaged.len();
                damaged[index] = (seed >> 24) as u8;
            }
            // Whatever still parses must also be written back unchanged.
            if let Ok(parsed) = PexFile::parse(&damaged) {
                assert_eq!(parsed.write()?, damaged, "{game}");
            }
            if let Ok(view) = PexView::parse(&damaged) {
                let _ = view.into_file();
            }
        }
    }
    Ok(())
}
