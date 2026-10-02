use super::super::*;
use super::shared::sample_cgb_envelope;

const FIRST_SQUARE1_DUTY_BYTE: usize = 5;
const FIRST_SQUARE2_DUTY_BYTE: usize = 4;

fn square1_with_duty(duty: u8) -> VoiceEntry {
    VoiceEntry::Square1(Square1Voice {
        base_key: 60,
        length: 0,
        sweep: 0,
        duty,
        envelope: sample_cgb_envelope(),
        fixed_rate: false,
    })
}

fn square2_with_duty(duty: u8) -> VoiceEntry {
    VoiceEntry::Square2(Square2Voice {
        base_key: 60,
        length: 0,
        duty,
        envelope: sample_cgb_envelope(),
        fixed_rate: false,
    })
}

#[test]
fn square_duty_boundaries_are_accepted_by_the_constructor_and_decode() {
    for duty in [0u8, 3] {
        let group =
            VoiceGroup::new(vec![square1_with_duty(duty), square2_with_duty(duty)]).unwrap();
        assert_eq!(VoiceGroup::decode(&group.encode()).unwrap(), group);
    }
}

#[test]
fn square_duty_out_of_range_is_rejected_at_construction() {
    for duty in [4u8, 255] {
        assert_eq!(
            VoiceGroup::new(vec![square1_with_duty(duty)]).unwrap_err(),
            AudioError::SquareDutyOutOfRange(duty)
        );
        assert_eq!(
            VoiceGroup::new(vec![square2_with_duty(duty)]).unwrap_err(),
            AudioError::SquareDutyOutOfRange(duty)
        );
    }
}

/// A duty byte above `3` aliases an in-domain value once
/// `audio::psg::SquareDuty::from_register` masks it with `0b11`, so decode rejects it.
#[test]
fn decode_rejects_out_of_range_square_duty() {
    let square1 = VoiceGroup::new(vec![square1_with_duty(2)]).unwrap();
    let mut bytes = square1.encode();
    bytes[FIRST_SQUARE1_DUTY_BYTE] = 4;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::SquareDutyOutOfRange(4))
    );

    let square2 = VoiceGroup::new(vec![square2_with_duty(2)]).unwrap();
    let mut bytes = square2.encode();
    bytes[FIRST_SQUARE2_DUTY_BYTE] = 4;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::SquareDutyOutOfRange(4))
    );
}
