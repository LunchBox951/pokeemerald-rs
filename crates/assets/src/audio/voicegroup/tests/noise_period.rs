use super::super::*;
use super::shared::{sample_cgb_envelope, MIDDLE_C};

const FIRST_NOISE_PERIOD_BYTE: usize = 4;
const MIN_NOISE_PERIOD: u8 = 0;
const MAX_VALID_NOISE_PERIOD: u8 = 1;
const FIRST_INVALID_NOISE_PERIOD: u8 = 2;
const MAX_BYTE_NOISE_PERIOD: u8 = u8::MAX;
const ALIASED_INVALID_NOISE_PERIOD: u8 = 3;

fn noise_with_period(period: u8) -> VoiceEntry {
    VoiceEntry::Noise(NoiseVoice {
        base_key: MIDDLE_C,
        length: 0,
        period,
        envelope: sample_cgb_envelope(),
        fixed_rate: false,
    })
}

#[test]
fn noise_period_boundaries_are_accepted_by_the_constructor_and_decode() {
    for period in [MIN_NOISE_PERIOD, MAX_VALID_NOISE_PERIOD] {
        let group = VoiceGroup::new(vec![noise_with_period(period)]).unwrap();
        assert_eq!(VoiceGroup::decode(&group.encode()).unwrap(), group);
    }
}

#[test]
fn noise_period_out_of_range_is_rejected_at_construction() {
    for period in [FIRST_INVALID_NOISE_PERIOD, MAX_BYTE_NOISE_PERIOD] {
        assert_eq!(
            VoiceGroup::new(vec![noise_with_period(period)]).unwrap_err(),
            AudioError::NoisePeriodOutOfRange(period)
        );
    }
}

#[test]
fn new_and_decode_reject_out_of_range_noise_period() {
    // Upstream `_voice_noise` stores `period & 1` (`asm/macros/music_voice.inc`) and the
    // runtime reads only bit 0 (`audio::cgb_voice::noise_control_byte`), so a period above
    // `1` aliases an in-domain selector and both `new` and `decode` must reject it.
    assert_eq!(
        VoiceGroup::new(vec![noise_with_period(FIRST_INVALID_NOISE_PERIOD)]).unwrap_err(),
        AudioError::NoisePeriodOutOfRange(FIRST_INVALID_NOISE_PERIOD)
    );

    let mut bytes = VoiceGroup::new(vec![noise_with_period(MAX_VALID_NOISE_PERIOD)])
        .unwrap()
        .encode();
    bytes[FIRST_NOISE_PERIOD_BYTE] = ALIASED_INVALID_NOISE_PERIOD;
    assert_eq!(
        VoiceGroup::decode(&bytes),
        Err(AudioError::NoisePeriodOutOfRange(
            ALIASED_INVALID_NOISE_PERIOD
        ))
    );
}
