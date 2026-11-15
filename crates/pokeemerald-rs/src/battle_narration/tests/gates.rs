//! Completion gates fail closed: a stage that waits on one never proceeds
//! while its text is missing, queued, printing, paused, prompting, or broken.

use engine::text::render::TextSpeed;

use super::fixtures::{bare_context, request};
use super::presentation_support::{
    layer, message, run_same, submit, text_fixture, trace, A_HELD, A_PRESS, NONE,
};
use crate::battle_narration::{
    AssetNames, BattleKind, GateError, GateStatus, LayerStatus, NarrationError, PresentationError,
    TemplateId,
};

/// The action, health-box, effects, capture, move-learning, and terminal-result
/// stages all proceed on exactly this.
fn may_proceed(status: GateStatus) -> bool {
    status.is_complete()
}

#[test]
fn a_prompt_holds_its_gate_against_held_buttons_until_a_fresh_press() {
    let fixture = text_fixture();
    for prompting in [
        message::WAIT_FOR_PRESS,
        message::PAGE_CLEAR,
        message::SCROLL,
    ] {
        let mut layer = layer(TextSpeed::Fast);
        let gate = submit(&mut layer, &fixture, &[prompting]);

        let held = run_same(&mut layer, 100, A_HELD);
        assert!(held.iter().all(|tick| tick.completed.is_none()));
        assert!(!may_proceed(layer.gate_status(&gate)));

        layer.tick(A_PRESS);
        run_same(&mut layer, 12, NONE);
        assert!(may_proceed(layer.gate_status(&gate)));
    }
}

#[test]
fn a_reserved_batch_blocks_itself_and_everything_after_it_until_its_producer_speaks() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let reserved = layer.reserve();
    let queued = submit(&mut layer, &fixture, &[message::AB]);

    let waiting = run_same(&mut layer, 30, A_PRESS);

    assert_eq!(layer.gate_status(&reserved), GateStatus::AwaitingProducer);
    assert_eq!(layer.gate_status(&queued), GateStatus::Pending);
    assert!(waiting
        .iter()
        .all(|tick| tick.status == LayerStatus::AwaitingProducer
            && tick.printer_event.is_none()
            && tick.completed.is_none()));

    layer
        .resolve(
            &reserved,
            &fixture.table(),
            &AssetNames,
            &super::presentation_support::requests(&[message::AB_AGAIN]),
        )
        .unwrap();
    let ticks = run_same(&mut layer, 6, NONE);

    assert_eq!(trace(&ticks), "GGFGGF");
    assert!(may_proceed(layer.gate_status(&reserved)));
    assert!(may_proceed(layer.gate_status(&queued)));
}

#[test]
fn a_batch_that_fails_to_expand_queues_nothing_and_blocks_everything_after_it() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let mut batch = super::presentation_support::requests(&[message::AB]);
    batch.push(request(
        TemplateId::Special("sText_NotInTheTable"),
        bare_context(BattleKind::Wild),
    ));
    let gate = layer.reserve();
    let queued_after = submit(&mut layer, &fixture, &[message::AB_AGAIN]);

    let error = layer
        .resolve(&gate, &fixture.table(), &AssetNames, &batch)
        .unwrap_err();

    assert_eq!(
        error,
        PresentationError::Expansion {
            request_index: 1,
            source: NarrationError::UnknownSpecial("sText_NotInTheTable".to_owned()),
        }
    );
    assert_eq!(layer.gate_status(&gate), GateStatus::Failed);
    let ticks = run_same(&mut layer, 20, NONE);
    assert!(ticks
        .iter()
        .all(|tick| tick.status == LayerStatus::Failed && tick.printer_event.is_none()));
    assert_eq!(layer.gate_status(&queued_after), GateStatus::Pending);
    assert!(!may_proceed(layer.gate_status(&gate)));
}

#[test]
fn an_empty_batch_completes_only_after_the_text_queued_ahead_of_it() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let text = submit(&mut layer, &fixture, &[message::AB]);
    let empty = submit(&mut layer, &fixture, &[]);

    let ticks = run_same(&mut layer, 4, NONE);

    let completed: Vec<_> = ticks.iter().map(|tick| tick.completed.clone()).collect();
    assert_eq!(completed, [None, None, Some(text), Some(empty)]);
}

#[test]
fn a_gate_resolves_once_and_only_in_the_layer_that_reserved_it() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let gate = submit(&mut layer, &fixture, &[message::AB]);
    let mut other = super::presentation_support::layer(TextSpeed::Fast);
    let foreign = other.reserve();

    let twice = layer.resolve(&gate, &fixture.table(), &AssetNames, &[]);
    // Same batch number as `gate`, issued by another layer.
    let wrong_layer = layer.resolve(&foreign, &fixture.table(), &AssetNames, &[]);

    assert_eq!(
        twice,
        Err(PresentationError::Gate(GateError::AlreadyResolved))
    );
    assert_eq!(
        wrong_layer,
        Err(PresentationError::Gate(GateError::Unknown))
    );
    assert_ne!(gate, foreign);
}

#[test]
fn another_layers_gate_never_reads_complete() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let own = submit(&mut layer, &fixture, &[message::AB]);
    let mut other = super::presentation_support::layer(TextSpeed::Fast);
    let foreign = other.reserve();

    run_same(&mut layer, 3, NONE);

    assert!(may_proceed(layer.gate_status(&own)));
    assert_eq!(layer.gate_status(&foreign), GateStatus::AwaitingProducer);
    assert_eq!(other.gate_status(&own), GateStatus::AwaitingProducer);
}

#[test]
fn the_reported_status_names_text_still_queued_after_a_message_ends() {
    let fixture = text_fixture();
    let mut layer = layer(TextSpeed::Fast);
    let reserved = layer.reserve();
    submit(&mut layer, &fixture, &[message::AB, message::AB_AGAIN]);
    layer
        .resolve(
            &reserved,
            &fixture.table(),
            &AssetNames,
            &super::presentation_support::requests(&[message::AB]),
        )
        .unwrap();

    let ticks = run_same(&mut layer, 9, NONE);

    let statuses: Vec<_> = ticks.iter().map(|tick| tick.status).collect();
    assert_eq!(
        statuses,
        [
            LayerStatus::Printing,
            LayerStatus::Printing,
            LayerStatus::Queued,
            LayerStatus::Printing,
            LayerStatus::Printing,
            LayerStatus::Queued,
            LayerStatus::Printing,
            LayerStatus::Printing,
            LayerStatus::Idle,
        ]
    );
}
