use std::fmt::Write as _;

use assets::pack::AssetPack;
use assets::{AnimCommand, AnimationSelector, AnimationTable, EffectSpriteId, PanSweep};

use super::*;

const HEADER: &str = "\
#ifndef GUARD
/* block
   comment */
#define ANIM_ATTACKER 0
#define ANIM_TARGET 1 // trailing
#define ANIM_SPRITES_START 10000
#define ANIM_TAG_A (ANIM_SPRITES_START + 3)
#define ANIM_TAG_LAST (ANIM_SPRITES_START + 288)
#define ANIM_TAG_PAST (ANIM_SPRITES_START + 289)
#define SE_X 134
#define PAN_T 63
#define ARG_RET_ID 15
#define RGB(r, g, b)  ((r) | ((g) << 5) | ((b) << 10))
#define RGB_WHITE RGB(31, 31, 31)
#define LONG 1 + \\
    2
#define CYCLE_A CYCLE_B
#define CYCLE_B CYCLE_A
enum Named
{
    E_A,
    E_B,
    E_C = 5,
    E_D
};
enum { ONE_LINE_A, ONE_LINE_B };
#endif
";
const ASSEMBLER: &str = ".set TRUE, 1\n.set FALSE, 0 @ comment\n";
const MACROS: &str = "\
@ test wrappers
	.macro hit anim_battler:req, x:req, y=3, tail
	.ifb \\tail
	createsprite gHit, \\anim_battler, 2, \\x, \\y
	.else
	createsprite gHit, \\anim_battler, 2, \\x, \\y, \\tail
	.endif
	.endm

	.macro scaled v:req
	createsprite gScaled, ANIM_ATTACKER, 2, 256 * \\v
	.endm

	.macro twice
	hit ANIM_TARGET, 1
	hit x=9, anim_battler=ANIM_ATTACKER, y=-4
	.endm

	.macro recursive
	recursive
	.endm

	.macro boom
	.error \"boom\"
	.endm

	.macro jumpreteq value:req, ptr:req
	jumpargeq ARG_RET_ID, \\value, \\ptr
	.endm

	.macro createsprite template:req, anim_battler:req, subpriority_offset:req, argv:vararg
	.byte 0x02
	.endm
";

fn script_sized(moves_first: &str, extra: &str, sizes: [usize; 4]) -> String {
    let symbols = [
        "gBattleAnims_Moves",
        "gBattleAnims_StatusConditions",
        "gBattleAnims_General",
        "gBattleAnims_Special",
    ];
    let mut text = String::from("#include \"constants/moves.h\"\n\t.section script_data\n");
    let mut generated = Vec::new();
    for (table, (symbol, size)) in symbols.iter().zip(sizes).enumerate() {
        writeln!(text, "\t.align 2\n{symbol}::").unwrap();
        for index in 0..size {
            let label = if table == 0 && index == 0 {
                moves_first.to_owned()
            } else if table == 0 && index == 355 {
                "Move_COUNT".to_owned()
            } else {
                let label = format!("Gen_{table}_{index}");
                generated.push(label.clone());
                label
            };
            writeln!(text, "\t.4byte {label} @ slot {index}").unwrap();
        }
    }
    for label in generated {
        writeln!(text, "{label}:\n\tend").unwrap();
    }
    text.push_str(extra);
    text
}

fn script(moves_first: &str, extra: &str) -> String {
    script_sized(moves_first, extra, [356, 9, 23, 7])
}

fn try_build(script: &str) -> Result<BattleAnimationCatalog, Fault> {
    let bytes = build(&Sources {
        script,
        macros: MACROS,
        headers: vec![("header.h", HEADER)],
        assembler: vec![("global.inc", ASSEMBLER)],
    })?;
    Ok(BattleAnimationCatalog::decode(&bytes).expect("the encoder's output decodes"))
}

fn catalog(script: &str) -> BattleAnimationCatalog {
    try_build(script).unwrap_or_else(|f| panic!("{}:{}: {}", f.file, f.line, f.detail))
}

fn rejection(script: &str) -> String {
    try_build(script)
        .expect_err("the source is malformed")
        .detail
}

fn program<'a>(catalog: &'a BattleAnimationCatalog, name: &str) -> &'a BattleAnimationProgram {
    catalog
        .programs()
        .iter()
        .find(|p| p.symbol().as_str() == name)
        .unwrap_or_else(|| panic!("no program {name}"))
}

fn commands(source: &str) -> Vec<AnimCommand> {
    let catalog = catalog(&script("Probe", &format!("Probe:\n{source}\n")));
    program(&catalog, "Probe").commands().to_vec()
}

fn id_of(catalog: &BattleAnimationCatalog, name: &str) -> ProgramId {
    let position = catalog
        .programs()
        .iter()
        .position(|p| p.symbol().as_str() == name)
        .unwrap();
    ProgramId::new(u16::try_from(position).unwrap())
}

fn selector(table: AnimationTable, index: u16) -> AnimationSelector {
    AnimationSelector::new(table, index).unwrap()
}

#[test]
fn every_selector_is_filled_in_source_slot_order() {
    let catalog = catalog(&script("Move_FIRST", "Move_FIRST:\n\tdelay 1\n\tend\n"));
    assert_eq!(catalog.programs().len(), 1 + 354 + 9 + 23 + 7);
    let first = catalog.program(selector(AnimationTable::Moves, 0));
    assert_eq!(first.symbol().as_str(), "Move_FIRST");
    for (table, prefix) in [
        (AnimationTable::Moves, "Gen_0_"),
        (AnimationTable::StatusConditions, "Gen_1_"),
        (AnimationTable::General, "Gen_2_"),
        (AnimationTable::Special, "Gen_3_"),
    ] {
        let first_generated = u16::from(table == AnimationTable::Moves);
        for index in first_generated..table.len() {
            let symbol = catalog.program(selector(table, index)).symbol().as_str();
            assert_eq!(symbol, format!("{prefix}{index}"), "{table} {index}");
        }
    }
}

#[test]
fn the_unreachable_move_count_entry_is_not_a_selector() {
    let catalog = catalog(&script("Move_FIRST", "Move_FIRST:\n\tend\n"));
    assert!(catalog
        .programs()
        .iter()
        .all(|p| p.symbol().as_str() != "Move_COUNT"));
}

#[test]
fn stacked_labels_become_distinct_programs_with_one_body() {
    let source = "Move_A:\nAlias:\nMove_B:\n\tdelay 4\n\tend\nProbe:\n\tcall Move_A\n\tcall Alias\n\tcall Move_B\n\tend\n";
    let catalog = catalog(&script("Probe", source));
    let (a, alias, b) = (
        program(&catalog, "Move_A"),
        program(&catalog, "Alias"),
        program(&catalog, "Move_B"),
    );
    assert_eq!(a.commands(), [AnimCommand::Delay(4), AnimCommand::End]);
    assert_eq!(a.commands(), alias.commands());
    assert_eq!(a.commands(), b.commands());
    assert_ne!(id_of(&catalog, "Move_A"), id_of(&catalog, "Move_B"));
}

#[test]
fn labels_link_to_program_ids_in_source_order() {
    let source = "\
Probe:
	call Helper
	jumpifmoveturn 3, Branch
	jumpifcontest Branch
	jumpargeq 2, -1, Helper
	choosetwoturnanim Branch, Helper
Helper:
	delay 1
	return
Branch:
	goto Helper
Unreached:
	end
";
    let catalog = catalog(&script("Probe", source));
    let (helper, branch) = (id_of(&catalog, "Helper"), id_of(&catalog, "Branch"));
    assert_eq!(
        program(&catalog, "Probe").commands(),
        [
            AnimCommand::Call(helper),
            AnimCommand::JumpIfMoveTurn {
                turn: 3,
                target: branch
            },
            AnimCommand::JumpIfContest(branch),
            AnimCommand::JumpArgEq {
                arg: 2,
                value: -1,
                target: helper
            },
            AnimCommand::ChooseTwoTurnAnim {
                even: branch,
                odd: helper
            },
        ]
    );
    assert_eq!(
        program(&catalog, "Branch").commands(),
        [AnimCommand::Goto(helper)]
    );
    assert!(catalog
        .programs()
        .iter()
        .all(|p| p.symbol().as_str() != "Unreached"));
    assert!(id_of(&catalog, "Probe") < helper && helper < branch);
}

#[test]
fn a_program_runs_through_an_intervening_label_to_its_first_terminator() {
    let source = "\
Probe:
	jumpargeq 15, 0, Tail
Tail:
	end
";
    let catalog = catalog(&script("Probe", source));
    assert_eq!(program(&catalog, "Probe").commands().len(), 2);
    assert_eq!(program(&catalog, "Tail").commands(), [AnimCommand::End]);
}

#[test]
fn commands_after_the_first_terminator_are_discarded() {
    let source = "\
Probe:
	choosetwoturnanim Probe, Probe
	end
Looping:
	delay 2
	goto Looping
	delay 3
	end
Returning:
	return
	end
Reach:
	call Probe
	call Looping
	call Returning
	end
";
    let catalog = catalog(&script("Reach", source));
    let id = |name| id_of(&catalog, name);
    assert_eq!(
        program(&catalog, "Probe").commands(),
        [AnimCommand::ChooseTwoTurnAnim {
            even: id("Probe"),
            odd: id("Probe")
        }]
    );
    assert_eq!(
        program(&catalog, "Looping").commands(),
        [AnimCommand::Delay(2), AnimCommand::Goto(id("Looping"))]
    );
    assert_eq!(
        program(&catalog, "Returning").commands(),
        [AnimCommand::Return]
    );
}

#[test]
fn sprite_and_task_identities_stay_symbolic() {
    let found = commands(
        "\tcreatesprite gFooSpriteTemplate, ANIM_TARGET, 5, 1, -2\n\
         \tcreatevisualtask AnimTask_Foo, 7, 65535, SE_X\n\
         \tcreatesoundtask SoundTask_Bar, 0\n\
         \tend",
    );
    assert_eq!(
        found,
        [
            AnimCommand::CreateSprite {
                template: AnimSymbol::new("gFooSpriteTemplate").unwrap(),
                relative_to_target: true,
                subpriority: 5,
                args: vec![1, -2],
            },
            AnimCommand::CreateVisualTask {
                task: AnimSymbol::new("AnimTask_Foo").unwrap(),
                priority: 7,
                args: vec![-1, 134],
            },
            AnimCommand::CreateSoundTask {
                task: AnimSymbol::new("SoundTask_Bar").unwrap(),
                args: vec![0],
            },
            AnimCommand::End,
        ]
    );
}

#[test]
fn sprite_tags_convert_to_effect_sprite_ids() {
    assert_eq!(
        commands("\tloadspritegfx ANIM_TAG_A\n\tunloadspritegfx ANIM_TAG_LAST\n\tend")[..2],
        [
            AnimCommand::LoadSpriteGfx(EffectSpriteId::new(3).unwrap()),
            AnimCommand::UnloadSpriteGfx(EffectSpriteId::new(288).unwrap()),
        ]
    );
    for bad in ["ANIM_TAG_PAST", "ANIM_ATTACKER"] {
        let detail = rejection(&script(
            "Probe",
            &format!("Probe:\n\tloadspritegfx {bad}\n\tend\n"),
        ));
        assert!(detail.contains("sprite"), "{detail}");
    }
}

#[test]
fn the_stock_out_of_bounds_jumpargeq_survives_and_ubfix_is_off() {
    let source = "\
.ifdef UBFIX
	jumpreteq FALSE, Probe
.else
	jumpargeq 15, FALSE, Probe
	jumpreteq TRUE, Probe
.endif
	end
";
    let found = commands(source);
    assert!(matches!(
        found[..],
        [
            AnimCommand::JumpArgEq {
                arg: 15,
                value: 0,
                ..
            },
            AnimCommand::JumpArgEq {
                arg: 15,
                value: 1,
                ..
            },
            AnimCommand::End
        ]
    ));
}

#[test]
fn wrapper_macros_bind_defaults_named_operands_and_blanks() {
    let found = commands("\ttwice\n\tend");
    let args = |command: &AnimCommand| match command {
        AnimCommand::CreateSprite { args, .. } => args.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(args(&found[0]), [1, 3]);
    assert_eq!(args(&found[1]), [9, -4]);
    let with_tail = commands("\thit ANIM_ATTACKER, 1, 2, 5\n\tend");
    assert_eq!(args(&with_tail[0]), [1, 2, 5]);
}

#[test]
fn macro_arguments_substitute_as_text_before_evaluation() {
    let found = commands("\tscaled 80/256\n\tscaled -3\n\tend");
    let arg = |command: &AnimCommand| match command {
        AnimCommand::CreateSprite { args, .. } => args[0],
        other => panic!("{other:?}"),
    };
    assert_eq!(arg(&found[0]), 80);
    assert_eq!(arg(&found[1]), -768);
}

#[test]
fn operands_follow_assembler_precedence_and_header_constants() {
    let found = commands(
        "\tdelay 1 | 2 + 4\n\
         \tdelay E_C + E_D\n\
         \tdelay ONE_LINE_B\n\
         \tsetbldcnt RGB_WHITE\n\
         \tdelay LONG\n\
         \tdelay 0x10 / 3 << 1\n\
         \tend",
    );
    assert_eq!(found[0], AnimCommand::Delay(7));
    assert_eq!(found[1], AnimCommand::Delay(11));
    assert_eq!(found[2], AnimCommand::Delay(1));
    assert_eq!(found[3], AnimCommand::SetBldCnt(0x7FFF));
    assert_eq!(found[4], AnimCommand::Delay(3));
    assert_eq!(found[5], AnimCommand::Delay(10));
}

#[test]
fn malformed_source_is_rejected() {
    let cases = [
        ("Probe:\n\tfrobnicate 1\n\tend\n", "unknown command"),
        ("Probe:\n\tgoto Nowhere\n\tend\n", "not defined"),
        ("Probe:\n\tdelay 1\n", "never terminates"),
        ("Probe:\n\tend\nProbe:\n\tend\n", "defined twice"),
        ("Probe:\n\t.byte 1\n\tend\n", "unsupported directive"),
        ("Probe:\n\tdelay 256\n\tend\n", "byte"),
        ("Probe:\n\tdelay\n\tend\n", "takes 1 operands"),
        (
            "Probe:\n\tcreatesprite gX, 0, 2, 1, 2, 3, 4, 5, 6, 7, 8, 9\n\tend\n",
            "Probe",
        ),
        ("Probe:\n\tboom\n\tend\n", "boom"),
        ("Probe:\n\trecursive\n\tend\n", "too deeply"),
        ("Probe:\n\tdelay CYCLE_A\n\tend\n", "too deeply"),
        ("Probe:\n\tdelay UNKNOWN\n\tend\n", "unknown constant"),
        ("Probe:\n\tdelay 1 +\n\tend\n", "ends early"),
        ("Probe:\n\tdelay 1/0\n\tend\n", "division by zero"),
        ("Probe:\n\tdelay (1 << 62) << 2\n\tend\n", "overflow"),
        (
            "Probe:\n\tloadspritegfx -9223372036854775807 - 1\n\tend\n",
            "overflows",
        ),
        ("Probe:\n\tgoto gBattleAnims_Moves\n\tend\n", "names data"),
        ("Probe:\n\tsetalpha 12, 256\n\tend\n", "unsigned byte"),
        ("Probe:\n\tcall 1abc\n\tend\n", "not a symbol"),
    ];
    for (extra, expected) in cases {
        let detail = rejection(&script("Probe", extra));
        assert!(detail.contains(expected), "{expected}: {detail}");
    }
}

#[test]
fn deeply_nested_expressions_are_rejected_not_overflowed() {
    let nested = format!("Probe:\n\tdelay {}1\n\tend\n", "-".repeat(20_000));
    assert!(rejection(&script("Probe", &nested)).contains("too deeply"));
}

#[test]
fn a_selector_on_a_label_stacked_over_data_is_rejected() {
    let source = script("Probe", "Probe:\nAlias:\n\t.2byte 0\nTail:\n\tend\n");
    assert!(rejection(&source).contains("names data"));
    let source = script("Probe", "Probe:\n\tdelay 1\n\t.2byte 0\n\tend\n");
    assert!(rejection(&source).contains("runs into data"));
}

#[test]
fn a_command_before_any_label_is_rejected() {
    assert!(rejection("\tdelay 1\n").contains("before any label"));
}

#[test]
fn table_sizes_and_the_move_sentinel_are_enforced() {
    let extra = "Probe:\n\tend\n";
    for sizes in [
        [355, 9, 23, 7],
        [357, 9, 23, 7],
        [356, 8, 23, 7],
        [356, 9, 24, 7],
        [356, 9, 23, 6],
    ] {
        let detail = rejection(&script_sized("Probe", extra, sizes));
        assert!(
            detail.contains("selectors") || detail.contains("Move_COUNT"),
            "{detail}"
        );
    }
    let detail = rejection(&script("Probe", extra).replace("Move_COUNT", "Gen_0_1"));
    assert!(detail.contains("must end with"), "{detail}");
    let detail = rejection(&script("Missing", ""));
    assert!(detail.contains("`Missing` is not defined"), "{detail}");
    let detail = rejection(&script("Probe", extra).replace("gBattleAnims_Special::", "other::"));
    assert!(detail.contains("outside a selector table"), "{detail}");
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one row per primitive; splitting would hide the 48-row total"
)]
fn every_primitive_converts_to_its_variant() {
    let sweep = PanSweep {
        se: 134,
        current_pan: -64,
        target_pan: 63,
        increment_pan: 2,
        delay: 3,
    };
    let zero = ProgramId::new(0);
    let sprite = EffectSpriteId::new(3).unwrap();
    let cases: Vec<(&str, &str, AnimCommand)> = vec![
        (
            "loadspritegfx",
            "ANIM_TAG_A",
            AnimCommand::LoadSpriteGfx(sprite),
        ),
        (
            "unloadspritegfx",
            "ANIM_TAG_A",
            AnimCommand::UnloadSpriteGfx(sprite),
        ),
        (
            "createsprite",
            "gT, ANIM_ATTACKER, 130, 1",
            AnimCommand::CreateSprite {
                template: AnimSymbol::new("gT").unwrap(),
                relative_to_target: false,
                subpriority: 2,
                args: vec![1],
            },
        ),
        (
            "createvisualtask",
            "T, 1, 2",
            AnimCommand::CreateVisualTask {
                task: AnimSymbol::new("T").unwrap(),
                priority: 1,
                args: vec![2],
            },
        ),
        ("delay", "9", AnimCommand::Delay(9)),
        ("waitforvisualfinish", "", AnimCommand::WaitForVisualFinish),
        ("nop", "", AnimCommand::Nop),
        ("nop2", "", AnimCommand::Nop2),
        ("end", "", AnimCommand::End),
        ("playse", "SE_X", AnimCommand::PlaySe(134)),
        ("monbg", "1", AnimCommand::MonBg(1)),
        ("clearmonbg", "1", AnimCommand::ClearMonBg(1)),
        (
            "setalpha",
            "12, 8",
            AnimCommand::SetAlpha { eva: 12, evb: 8 },
        ),
        ("blendoff", "", AnimCommand::BlendOff),
        ("call", "L", AnimCommand::Call(zero)),
        ("return", "", AnimCommand::Return),
        ("setarg", "7, -1", AnimCommand::SetArg { arg: 7, value: -1 }),
        (
            "choosetwoturnanim",
            "L, M",
            AnimCommand::ChooseTwoTurnAnim {
                even: zero,
                odd: zero,
            },
        ),
        (
            "jumpifmoveturn",
            "1, L",
            AnimCommand::JumpIfMoveTurn {
                turn: 1,
                target: zero,
            },
        ),
        ("goto", "L", AnimCommand::Goto(zero)),
        ("fadetobg", "2", AnimCommand::FadeToBg(2)),
        ("restorebg", "", AnimCommand::RestoreBg),
        ("waitbgfadeout", "", AnimCommand::WaitBgFadeOut),
        ("waitbgfadein", "", AnimCommand::WaitBgFadeIn),
        ("changebg", "3", AnimCommand::ChangeBg(3)),
        (
            "playsewithpan",
            "SE_X, PAN_T",
            AnimCommand::PlaySeWithPan { se: 134, pan: 63 },
        ),
        ("setpan", "-64", AnimCommand::SetPan(-64)),
        ("panse", "SE_X, -64, 63, 2, 3", AnimCommand::PanSe(sweep)),
        (
            "loopsewithpan",
            "SE_X, PAN_T, 8, 10",
            AnimCommand::LoopSeWithPan {
                se: 134,
                pan: 63,
                wait: 8,
                times: 10,
            },
        ),
        (
            "waitplaysewithpan",
            "SE_X, PAN_T, 4",
            AnimCommand::WaitPlaySeWithPan {
                se: 134,
                pan: 63,
                wait: 4,
            },
        ),
        ("setbldcnt", "0x3F40", AnimCommand::SetBldCnt(0x3F40)),
        (
            "createsoundtask",
            "S, 1, 2",
            AnimCommand::CreateSoundTask {
                task: AnimSymbol::new("S").unwrap(),
                args: vec![1, 2],
            },
        ),
        ("waitsound", "", AnimCommand::WaitSound),
        (
            "jumpargeq",
            "15, TRUE, L",
            AnimCommand::JumpArgEq {
                arg: 15,
                value: 1,
                target: zero,
            },
        ),
        ("monbg_static", "1", AnimCommand::MonBgStatic(1)),
        ("clearmonbg_static", "1", AnimCommand::ClearMonBgStatic(1)),
        ("jumpifcontest", "L", AnimCommand::JumpIfContest(zero)),
        (
            "fadetobgfromset",
            "1, 2, 3",
            AnimCommand::FadeToBgFromSet {
                opponent: 1,
                player: 2,
                contest: 3,
            },
        ),
        (
            "panse_adjustnone",
            "SE_X, -64, 63, 2, 3",
            AnimCommand::PanSeAdjustNone(sweep),
        ),
        (
            "panse_adjustall",
            "SE_X, -64, 63, 2, 3",
            AnimCommand::PanSeAdjustAll(sweep),
        ),
        ("splitbgprio", "1", AnimCommand::SplitBgPrio(1)),
        ("splitbgprio_all", "", AnimCommand::SplitBgPrioAll),
        ("splitbgprio_foes", "1", AnimCommand::SplitBgPrioFoes(1)),
        ("invisible", "1", AnimCommand::Invisible(1)),
        ("visible", "1", AnimCommand::Visible(1)),
        (
            "teamattack_moveback",
            "1",
            AnimCommand::TeamAttackMoveBack(1),
        ),
        ("teamattack_movefwd", "1", AnimCommand::TeamAttackMoveFwd(1)),
        ("stopsound", "", AnimCommand::StopSound),
    ];
    assert_eq!(cases.len(), commands::PRIMITIVES.len());
    let mut constants = Constants::default();
    constants.load_header("header.h", HEADER).unwrap();
    constants.load_assembler("global.inc", ASSEMBLER).unwrap();
    for (name, operands, expected) in cases {
        assert!(commands::PRIMITIVES.contains(&name), "{name}");
        let args = macros::split_arguments(operands).unwrap();
        let pending = commands::convert(name, &args, &constants)
            .expect("a primitive")
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let ids = vec![zero; pending.targets.len()];
        assert_eq!(pending.link(&ids), expected, "{name}");
    }
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn battle_anim_catalog_is_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-battle-anim-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let catalog = pack.battle_animation_catalog().unwrap();

    assert_eq!(catalog.programs().len(), 650);
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            let _ = catalog.program(selector(table, index));
        }
    }
    let name_of = |table, index| catalog.program(selector(table, index)).symbol().as_str();
    assert_eq!(name_of(AnimationTable::Moves, 0), "Move_NONE");
    assert_eq!(name_of(AnimationTable::Moves, 1), "Move_POUND");
    assert_eq!(name_of(AnimationTable::Moves, 354), "Move_PSYCHO_BOOST");
    assert_eq!(
        name_of(AnimationTable::StatusConditions, 0),
        "Status_Poison"
    );
    assert_eq!(name_of(AnimationTable::General, 22), "General_WishHeal");
    assert_eq!(
        name_of(AnimationTable::Special, 6),
        "Special_MonToSubstitute"
    );

    let magnitude = program(&catalog, "Move_MAGNITUDE");
    assert!(magnitude
        .commands()
        .iter()
        .any(|c| matches!(c, AnimCommand::JumpArgEq { arg: 15, .. })));
    assert_eq!(program(&catalog, "Move_BIDE").commands().len(), 1);
    let pound = program(&catalog, "Move_POUND");
    assert_eq!(pound.commands().last(), Some(&AnimCommand::End));
    assert_eq!(
        pound.commands(),
        program(&catalog, "Move_MIRROR_MOVE").commands()
    );
    assert!(catalog
        .programs()
        .iter()
        .all(|p| p.symbol().as_str() != "Move_COUNT"));
}
