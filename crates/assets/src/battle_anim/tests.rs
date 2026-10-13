use super::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationError, BattleAnimationProgram, EffectSpriteId, PanSweep, ProgramId,
    ANIM_ARGS_COUNT, EFFECT_SPRITE_COUNT,
};

fn symbol(name: &str) -> AnimSymbol {
    AnimSymbol::new(name).unwrap()
}

fn program(name: &str, commands: Vec<AnimCommand>) -> BattleAnimationProgram {
    BattleAnimationProgram::new(symbol(name), commands).unwrap()
}

fn selector(table: AnimationTable, index: u16) -> AnimationSelector {
    AnimationSelector::new(table, index).unwrap()
}

/// Every command, so a round trip exercises each opcode.
fn every_command() -> Vec<AnimCommand> {
    let sweep = PanSweep {
        se: 0x1234,
        current_pan: -64,
        target_pan: 63,
        increment_pan: -3,
        delay: 2,
    };
    let sprite = EffectSpriteId::new(EFFECT_SPRITE_COUNT - 1).unwrap();
    vec![
        AnimCommand::LoadSpriteGfx(sprite),
        AnimCommand::UnloadSpriteGfx(EffectSpriteId::new(0).unwrap()),
        AnimCommand::CreateSprite {
            template: symbol("gSimplePaletteBlendSpriteTemplate"),
            relative_to_target: true,
            subpriority: 0x7F,
            args: vec![-1, 0, 32767, -32768],
        },
        AnimCommand::CreateVisualTask {
            task: symbol("AnimTask_HorizontalShake"),
            priority: 5,
            args: vec![1; ANIM_ARGS_COUNT],
        },
        AnimCommand::Delay(0),
        AnimCommand::WaitForVisualFinish,
        AnimCommand::Nop,
        AnimCommand::Nop2,
        AnimCommand::PlaySe(0xFFFF),
        AnimCommand::MonBg(1),
        AnimCommand::ClearMonBg(1),
        AnimCommand::SetAlpha { eva: 12, evb: 8 },
        AnimCommand::BlendOff,
        AnimCommand::Call(ProgramId::new(1)),
        AnimCommand::SetArg { arg: 7, value: -2 },
        AnimCommand::JumpIfMoveTurn {
            turn: 1,
            target: ProgramId::new(1),
        },
        AnimCommand::FadeToBg(3),
        AnimCommand::RestoreBg,
        AnimCommand::WaitBgFadeOut,
        AnimCommand::WaitBgFadeIn,
        AnimCommand::ChangeBg(4),
        AnimCommand::PlaySeWithPan { se: 9, pan: -64 },
        AnimCommand::SetPan(63),
        AnimCommand::PanSe(sweep),
        AnimCommand::LoopSeWithPan {
            se: 1,
            pan: 2,
            wait: 3,
            times: 4,
        },
        AnimCommand::WaitPlaySeWithPan {
            se: 5,
            pan: -6,
            wait: 7,
        },
        AnimCommand::SetBldCnt(0x3F40),
        AnimCommand::CreateSoundTask {
            task: symbol("SoundTask_PlayDoubleCry"),
            args: vec![],
        },
        AnimCommand::WaitSound,
        AnimCommand::JumpArgEq {
            arg: 0,
            value: 4,
            target: ProgramId::new(1),
        },
        AnimCommand::MonBgStatic(0),
        AnimCommand::ClearMonBgStatic(0),
        AnimCommand::JumpIfContest(ProgramId::new(1)),
        AnimCommand::FadeToBgFromSet {
            opponent: 1,
            player: 2,
            contest: 3,
        },
        AnimCommand::PanSeAdjustNone(sweep),
        AnimCommand::PanSeAdjustAll(sweep),
        AnimCommand::SplitBgPrio(2),
        AnimCommand::SplitBgPrioAll,
        AnimCommand::SplitBgPrioFoes(3),
        AnimCommand::Invisible(0),
        AnimCommand::Visible(0),
        AnimCommand::TeamAttackMoveBack(0),
        AnimCommand::TeamAttackMoveFwd(1),
        AnimCommand::StopSound,
        AnimCommand::ChooseTwoTurnAnim {
            even: ProgramId::new(0),
            odd: ProgramId::new(1),
        },
    ]
}

fn programs() -> Vec<BattleAnimationProgram> {
    vec![
        program("Move_POUND", every_command()),
        program("Common_Finish", vec![AnimCommand::End]),
    ]
}

/// Every slot of every table, by default the finishing program.
fn all_selectors(custom: &[(AnimationSelector, u16)]) -> Vec<(AnimationSelector, ProgramId)> {
    AnimationTable::ALL
        .into_iter()
        .flat_map(|table| (0..table.len()).map(move |index| selector(table, index)))
        .map(|slot| {
            let id = custom
                .iter()
                .find(|(chosen, _)| *chosen == slot)
                .map_or(1, |(_, id)| *id);
            (slot, ProgramId::new(id))
        })
        .collect()
}

fn catalog() -> BattleAnimationCatalog {
    let pound = selector(AnimationTable::Moves, 1);
    BattleAnimationCatalog::new(programs(), all_selectors(&[(pound, 0)])).unwrap()
}

#[test]
fn table_sizes_are_the_upstream_counts() {
    let sizes = AnimationTable::ALL.map(AnimationTable::len);
    assert_eq!(sizes, [355, 9, 23, 7]);
}

#[test]
fn selector_lookup_resolves_each_table_to_its_program() {
    let catalog = catalog();
    assert_eq!(
        catalog
            .program(selector(AnimationTable::Moves, 1))
            .symbol()
            .as_str(),
        "Move_POUND"
    );
    for table in AnimationTable::ALL {
        let last = selector(table, table.len() - 1);
        assert_eq!(catalog.program(last).symbol().as_str(), "Common_Finish");
    }
    assert_eq!(
        catalog
            .program(selector(AnimationTable::Moves, 1))
            .commands(),
        every_command()
    );
    assert_eq!(
        catalog
            .program_by_id(ProgramId::new(1))
            .unwrap()
            .symbol()
            .as_str(),
        "Common_Finish"
    );
    assert!(catalog.program_by_id(ProgramId::new(2)).is_none());
}

#[test]
fn round_trip_is_lossless_and_encoding_is_deterministic() {
    let catalog = catalog();
    let bytes = catalog.encode();
    assert_eq!(bytes, catalog.encode());
    let decoded = BattleAnimationCatalog::decode(&bytes).unwrap();
    assert_eq!(decoded, catalog);
    assert_eq!(decoded.encode(), bytes);
}

#[test]
fn selector_order_at_construction_does_not_change_the_encoding() {
    let pound = selector(AnimationTable::Moves, 1);
    let mut reversed = all_selectors(&[(pound, 0)]);
    reversed.reverse();
    let other = BattleAnimationCatalog::new(programs(), reversed).unwrap();
    assert_eq!(other.encode(), catalog().encode());
}

#[test]
fn encoding_pins_a_minimal_catalog() {
    let catalog = BattleAnimationCatalog::new(vec![program("A", vec![AnimCommand::End])], {
        all_selectors(&[])
            .into_iter()
            .map(|(slot, _)| (slot, ProgramId::new(0)))
    })
    .unwrap();
    let bytes = catalog.encode();
    // version 1, one program, symbol "A", one command: end (0x08).
    assert_eq!(bytes[..10], [1, 0, 1, 0, 1, 0, b'A', 1, 0, 0x08]);
    assert_eq!(bytes.len(), 10 + 2 * (355 + 9 + 23 + 7));
}

#[test]
fn every_truncation_is_rejected() {
    let bytes = catalog().encode();
    for len in 0..bytes.len() {
        assert!(
            BattleAnimationCatalog::decode(&bytes[..len]).is_err(),
            "prefix {len} decoded"
        );
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = catalog().encode();
    bytes.push(0);
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::TrailingBytes(1))
    );
}

#[test]
fn an_unknown_schema_version_is_rejected() {
    let mut bytes = catalog().encode();
    bytes[0] = 2;
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::UnsupportedVersion(2))
    );
}

#[test]
fn an_unknown_opcode_is_rejected() {
    let mut bytes = catalog().encode();
    // The first command of the first program follows version, count, symbol, count.
    let at = 2 + 2 + 2 + "Move_POUND".len() + 2;
    bytes[at] = 0x30;
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::UnknownOpcode(0x30))
    );
}

#[test]
fn an_out_of_range_sprite_on_the_wire_is_rejected() {
    let mut bytes = catalog().encode();
    let at = 2 + 2 + 2 + "Move_POUND".len() + 2 + 1;
    bytes[at..at + 2].copy_from_slice(&EFFECT_SPRITE_COUNT.to_le_bytes());
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::SpriteIdOutOfRange(
            EFFECT_SPRITE_COUNT
        ))
    );
}

#[test]
fn a_noncanonical_target_flag_is_rejected() {
    let single = BattleAnimationProgram::new(
        symbol("A"),
        vec![
            AnimCommand::CreateSprite {
                template: symbol("T"),
                relative_to_target: false,
                subpriority: 2,
                args: vec![],
            },
            AnimCommand::End,
        ],
    )
    .unwrap();
    let catalog = BattleAnimationCatalog::new(
        vec![single],
        all_selectors(&[])
            .into_iter()
            .map(|(slot, _)| (slot, ProgramId::new(0))),
    )
    .unwrap();
    let mut bytes = catalog.encode();
    // version, count, symbol "A", command count, opcode, template "T", then the flag.
    let at = 2 + 2 + 3 + 2 + 1 + 3;
    assert_eq!(bytes[at], 0);
    bytes[at] = 2;
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::InvalidFlag(2))
    );
}

#[test]
fn symbols_must_be_c_identifiers() {
    for bad in ["", "9bad", "has space", "dash-ed", "é", "new\nline"] {
        assert_eq!(
            AnimSymbol::new(bad),
            Err(BattleAnimationError::InvalidSymbol)
        );
    }
    assert_eq!(
        AnimSymbol::new(&"a".repeat(70_000)),
        Err(BattleAnimationError::InvalidSymbol)
    );
    assert!(AnimSymbol::new("AnimTask_Foo2").is_ok());
}

#[test]
fn a_non_utf8_symbol_on_the_wire_is_rejected() {
    let mut bytes = catalog().encode();
    bytes[6] = 0xFF;
    assert_eq!(
        BattleAnimationCatalog::decode(&bytes),
        Err(BattleAnimationError::InvalidSymbol)
    );
}

#[test]
fn selectors_are_range_checked() {
    for table in AnimationTable::ALL {
        assert!(AnimationSelector::new(table, table.len() - 1).is_ok());
        assert_eq!(
            AnimationSelector::new(table, table.len()),
            Err(BattleAnimationError::SelectorOutOfRange {
                table,
                index: table.len()
            })
        );
    }
}

#[test]
fn effect_sprites_are_range_checked() {
    assert!(EffectSpriteId::new(EFFECT_SPRITE_COUNT - 1).is_ok());
    assert_eq!(
        EffectSpriteId::new(EFFECT_SPRITE_COUNT),
        Err(BattleAnimationError::SpriteIdOutOfRange(
            EFFECT_SPRITE_COUNT
        ))
    );
}

#[test]
fn malformed_programs_are_rejected() {
    let build = |commands| BattleAnimationProgram::new(symbol("P"), commands);
    assert_eq!(build(vec![]), Err(BattleAnimationError::EmptyProgram));
    assert_eq!(
        build(vec![AnimCommand::Nop]),
        Err(BattleAnimationError::MissingTerminator)
    );
    let sprite = |subpriority, args| AnimCommand::CreateSprite {
        template: symbol("T"),
        relative_to_target: false,
        subpriority,
        args,
    };
    assert_eq!(
        build(vec![sprite(0x80, vec![]), AnimCommand::End]),
        Err(BattleAnimationError::SubpriorityOutOfRange(0x80))
    );
    assert_eq!(
        build(vec![sprite(0, vec![0; 9]), AnimCommand::End]),
        Err(BattleAnimationError::ArgCountTooLarge(9))
    );
    for terminator in [
        AnimCommand::End,
        AnimCommand::Return,
        AnimCommand::Goto(ProgramId::new(0)),
        AnimCommand::ChooseTwoTurnAnim {
            even: ProgramId::new(0),
            odd: ProgramId::new(0),
        },
    ] {
        assert!(build(vec![terminator]).is_ok());
    }
}

#[test]
fn a_catalog_without_a_selector_has_no_fallback() {
    let mut selectors = all_selectors(&[]);
    let missing = selectors.pop().unwrap().0;
    assert_eq!(
        BattleAnimationCatalog::new(programs(), selectors),
        Err(BattleAnimationError::MissingSelector(missing))
    );
}

#[test]
fn a_repeated_selector_is_rejected() {
    let mut selectors = all_selectors(&[]);
    let repeated = selectors[0];
    selectors.push(repeated);
    assert_eq!(
        BattleAnimationCatalog::new(programs(), selectors),
        Err(BattleAnimationError::DuplicateSelector(repeated.0))
    );
}

#[test]
fn dangling_program_references_are_rejected() {
    let dangling = |commands| {
        let programs = vec![program("A", commands)];
        BattleAnimationCatalog::new(
            programs,
            all_selectors(&[])
                .into_iter()
                .map(|(slot, _)| (slot, ProgramId::new(0))),
        )
    };
    let error = Err(BattleAnimationError::ProgramOutOfRange { id: 5, count: 1 });
    assert_eq!(dangling(vec![AnimCommand::Goto(ProgramId::new(5))]), error);
    assert_eq!(
        dangling(vec![AnimCommand::ChooseTwoTurnAnim {
            even: ProgramId::new(0),
            odd: ProgramId::new(5),
        }]),
        error
    );
    assert_eq!(
        BattleAnimationCatalog::new(
            programs(),
            all_selectors(&[])
                .into_iter()
                .map(|(slot, _)| (slot, ProgramId::new(5))),
        ),
        Err(BattleAnimationError::ProgramOutOfRange { id: 5, count: 2 })
    );
}

#[test]
fn duplicate_labels_are_rejected() {
    let twin = vec![
        program("Same", vec![AnimCommand::End]),
        program("Same", vec![AnimCommand::Return]),
    ];
    assert_eq!(
        BattleAnimationCatalog::new(twin, all_selectors(&[])),
        Err(BattleAnimationError::DuplicateSymbol("Same".to_owned()))
    );
}

#[test]
fn errors_display_their_cause() {
    let rendered =
        BattleAnimationError::MissingSelector(selector(AnimationTable::Special, 3)).to_string();
    assert_eq!(rendered, "special animation 3 has no program");
}

#[test]
fn argument_indexes_stay_raw_like_stock_move_magnitude() {
    // Stock `Move_MAGNITUDE` jumps on argument 15 (`data/battle_anim_scripts.s:3324-3326`).
    let commands = vec![
        AnimCommand::SetArg { arg: 200, value: 1 },
        AnimCommand::JumpArgEq {
            arg: 15,
            value: 0,
            target: ProgramId::new(0),
        },
        AnimCommand::End,
    ];
    let magnitude = program("Move_MAGNITUDE", commands.clone());
    let catalog = BattleAnimationCatalog::new(
        vec![magnitude],
        all_selectors(&[])
            .into_iter()
            .map(|(slot, _)| (slot, ProgramId::new(0))),
    )
    .unwrap();
    let decoded = BattleAnimationCatalog::decode(&catalog.encode()).unwrap();
    assert_eq!(decoded.programs()[0].commands(), commands);
}
