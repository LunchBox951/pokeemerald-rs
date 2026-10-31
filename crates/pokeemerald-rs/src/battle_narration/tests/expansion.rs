//! Placeholder and buffer expansion, `BattleStringExpandPlaceholders`
//! (`src/battle_message.c:2312`-`:2764`).

use assets::items::ItemId;
use assets::MoveId;
use engine::text::Token;

use super::fixtures::{bare_context, render, request, Fixture, RATTATA};
use crate::battle_narration::{
    expand, AssetNames, BattleBuffer, BattleKind, Battler, BufferOp, NarrationError, Side,
    StatusName, TemplateId,
};

fn expanded(
    fixture: &Fixture,
    id: u16,
    context: crate::battle_narration::NarrationContext,
) -> String {
    let tokens = expand(
        &fixture.table(),
        &AssetNames,
        &request(TemplateId::Table(id), context),
    )
    .unwrap();
    render(&tokens)
}

#[test]
fn a_buffer_program_appends_entries_and_the_next_template_slot_reads_the_copy() {
    let fixture = Fixture::new(
        &[
            (20, "{B_BUFF1},{STR_VAR_1}"),
            (21, "harshly "),
            (22, "fell!"),
        ],
        &[],
    );
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Program(vec![BufferOp::String(21), BufferOp::String(22)]);

    assert_eq!(
        expanded(&fixture, 20, context),
        "harshly fell!,harshly fell!"
    );
}

#[test]
fn a_string_variable_read_before_its_buffer_keeps_the_snapshotted_text() {
    let fixture = Fixture::new(
        &[(20, "{STR_VAR_1}>{B_BUFF1}>{STR_VAR_1}"), (21, "new")],
        &[],
    );
    let mut context = bare_context(BattleKind::Wild);
    context.string_vars[0] = vec![Token::Char('o'), Token::Char('l'), Token::Char('d')];
    context.buffers[0] = BattleBuffer::Program(vec![BufferOp::String(21)]);

    assert_eq!(expanded(&fixture, 20, context), "old>new>new");
}

#[test]
fn a_table_string_inside_a_buffer_keeps_its_own_placeholders_unexpanded() {
    let fixture = Fixture::new(
        &[(20, "<{B_BUFF2}>"), (21, "{B_PLAYER_NAME} sees {B_BUFF1}")],
        &[],
    );
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Text("inner".to_owned());
    context.buffers[1] = BattleBuffer::Program(vec![BufferOp::String(21)]);

    assert_eq!(
        expanded(&fixture, 20, context),
        "<{PLACEHOLDER:0x23} sees {PLACEHOLDER:0x00}>",
        "the copy step does not re-enter the expander"
    );
}

#[test]
fn number_species_and_nickname_entries_replace_what_came_before() {
    let fixture = Fixture::new(&[(20, "{B_BUFF1}/{B_BUFF2}/{B_BUFF3}"), (21, "x")], &[]);
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Program(vec![
        BufferOp::String(21),
        BufferOp::Number {
            value: 42,
            width: 5,
        },
    ]);
    context.buffers[1] =
        BattleBuffer::Program(vec![BufferOp::String(21), BufferOp::Species(RATTATA)]);
    context.buffers[2] = BattleBuffer::Program(vec![
        BufferOp::String(21),
        BufferOp::MonNick("ACE".to_owned()),
        BufferOp::String(21),
    ]);

    assert_eq!(expanded(&fixture, 20, context), "42/RATTATA/ACEx");
}

#[test]
fn move_type_stat_ability_and_flavor_entries_append() {
    let fixture = Fixture::new(&[(20, "{B_BUFF1}")], &[]);
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Program(vec![
        BufferOp::Move(MoveId::TACKLE),
        BufferOp::Type(3),
        BufferOp::Stat(4),
        BufferOp::Ability(assets::AbilityId::LEVITATE),
        BufferOp::NegativeFlavor(2),
    ]);

    assert_eq!(
        expanded(&fixture, 20, context),
        "TACKLEPOISONSP. ATKLEVITATEwas too sweet!"
    );
}

#[test]
fn an_item_entry_replaces_what_came_before_it() {
    let fixture = Fixture::new(&[(20, "{B_BUFF1}")], &[]);
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Program(vec![
        BufferOp::Move(MoveId::TACKLE),
        BufferOp::Item(ItemId(13)),
        BufferOp::NegativeFlavor(2),
    ]);

    assert_eq!(expanded(&fixture, 20, context), "POTIONwas too sweet!");
}

#[test]
fn a_nickname_with_prefix_names_the_battle_kind_for_opposing_battlers_only() {
    let fixture = Fixture::new(&[(20, "{B_BUFF1}")], &[]);
    let program = |kind, side| {
        let mut context = bare_context(kind);
        context.buffers[0] = BattleBuffer::Program(vec![BufferOp::MonNickWithPrefix {
            side,
            name: "ACE".to_owned(),
        }]);
        expanded(&fixture, 20, context)
    };

    assert_eq!(program(BattleKind::Wild, Side::Opponent), "Wild ACE");
    assert_eq!(program(BattleKind::Trainer, Side::Opponent), "Foe ACE");
    assert_eq!(program(BattleKind::Trainer, Side::Player), "ACE");
}

#[test]
fn buffer_one_names_a_status_and_the_other_buffers_refuse_one() {
    let fixture = Fixture::new(&[(20, "{B_BUFF1}"), (21, "{B_BUFF2}")], &[]);
    let mut context = bare_context(BattleKind::Wild);
    context.buffers[0] = BattleBuffer::Status(StatusName::Paralysis);
    context.buffers[1] = BattleBuffer::Status(StatusName::Paralysis);

    assert_eq!(expanded(&fixture, 20, context.clone()), "paralysis");
    assert_eq!(
        expand(
            &fixture.table(),
            &AssetNames,
            &request(TemplateId::Table(21), context)
        ),
        Err(NarrationError::StatusOutsideBuffer1)
    );
}

#[test]
fn an_attacker_prefix_and_ability_read_the_snapshotted_roles() {
    let fixture = Fixture::new(
        &[(20, "{B_ATK_PREFIX1}/{B_DEF_PREFIX1}/{B_ATK_ABILITY}")],
        &[],
    );
    let mut context = bare_context(BattleKind::Wild);
    context.player = Some(Battler {
        name: "TORCHIC".to_owned(),
        ability: Some(assets::AbilityId::LEVITATE),
    });
    context.opponent = Some(Battler {
        name: "RATTATA".to_owned(),
        ability: None,
    });
    context.attacker = Some(Side::Player);
    context.defender = Some(Side::Opponent);

    assert_eq!(expanded(&fixture, 20, context), "Ally/Foe/LEVITATE");
}

#[test]
fn a_defeat_text_ends_with_a_wait_for_input() {
    let fixture = Fixture::new(&[(20, "{B_TRAINER1_LOSE_TEXT}")], &[]);
    let context = super::fixtures::scene(BattleKind::Trainer);
    let narrator = crate::battle_narration::Narrator::new(context);
    let mut request = narrator.introduction();
    request.template = TemplateId::Table(20);

    let tokens = expand(&fixture.table(), &AssetNames, &request).unwrap();

    assert_eq!(render(&tokens), "Good battle!{CTRL:0x09}");
}

#[test]
fn the_type_move_string_stands_in_for_a_move_outside_the_move_table() {
    let fixture = Fixture::new(
        &[(20, "{B_CURRENT_MOVE}")],
        &[("sATypeMove_Table[TYPE_FIRE]", "FIRE move")],
    );
    let mut context = bare_context(BattleKind::Wild);
    context.move_type = 10;

    assert_eq!(expanded(&fixture, 20, context), "FIRE move");
}

#[test]
fn a_template_reading_an_unset_role_fails_instead_of_printing_nothing() {
    let fixture = Fixture::new(&[(20, "{B_ATK_NAME_WITH_PREFIX}")], &[]);

    assert_eq!(
        expand(
            &fixture.table(),
            &AssetNames,
            &request(TemplateId::Table(20), bare_context(BattleKind::Wild))
        ),
        Err(NarrationError::MissingContext("attacker"))
    );
}

#[test]
fn a_placeholder_only_other_battle_types_reach_is_refused() {
    let fixture = Fixture::new(&[(20, "{B_LINK_PLAYER_MON1_NAME}")], &[]);

    assert_eq!(
        expand(
            &fixture.table(),
            &AssetNames,
            &request(TemplateId::Table(20), bare_context(BattleKind::Wild))
        ),
        Err(NarrationError::UnsupportedPlaceholder(0x09))
    );
}

#[test]
fn controls_and_the_terminator_survive_expansion() {
    let fixture = Fixture::new(&[(20, "a\nb\\p{B_BUFF1}")], &[]);
    let tokens = expand(
        &fixture.table(),
        &AssetNames,
        &request(TemplateId::Table(20), bare_context(BattleKind::Wild)),
    )
    .unwrap();

    assert_eq!(
        tokens,
        vec![
            Token::Char('a'),
            Token::Newline,
            Token::Char('b'),
            Token::PromptScroll,
            Token::End,
        ]
    );
}
