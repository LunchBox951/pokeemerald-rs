use super::super::*;
use super::shared::MIDDLE_C;

const CGB_ENVELOPE_MIN: Envelope = Envelope {
    attack: 0,
    decay: 0,
    sustain: 0,
    release: 0,
};
const CGB_ENVELOPE_MAX: Envelope = Envelope {
    attack: MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE,
    decay: MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE,
    sustain: MAX_CGB_ENVELOPE_SUSTAIN,
    release: MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE,
};

fn square1_with_envelope(envelope: Envelope) -> VoiceEntry {
    VoiceEntry::Square1(Square1Voice {
        base_key: 60,
        length: 0,
        sweep: 0,
        duty: 0,
        envelope,
        fixed_rate: false,
    })
}

fn square2_with_envelope(envelope: Envelope) -> VoiceEntry {
    VoiceEntry::Square2(Square2Voice {
        base_key: 60,
        length: 0,
        duty: 0,
        envelope,
        fixed_rate: false,
    })
}

fn programmable_wave_with_envelope(envelope: Envelope) -> VoiceEntry {
    VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
        base_key: 60,
        length: 0,
        wave: SampleId("audio/sample/programmable_wave_2".to_owned()),
        envelope,
        fixed_rate: false,
    })
}

fn noise_with_envelope(envelope: Envelope) -> VoiceEntry {
    VoiceEntry::Noise(NoiseVoice {
        base_key: MIDDLE_C,
        length: 0,
        period: 0,
        envelope,
        fixed_rate: false,
    })
}

type CgbVoiceBuilder = fn(Envelope) -> VoiceEntry;

/// Every CGB voice kind paired with the fixture builder that carries its
/// envelope, so envelope-domain coverage does not silently skip one kind.
fn cgb_voice_kinds_with_envelope() -> [(&'static str, CgbVoiceBuilder); 4] {
    [
        ("square 1", square1_with_envelope as CgbVoiceBuilder),
        ("square 2", square2_with_envelope),
        ("programmable wave", programmable_wave_with_envelope),
        ("noise", noise_with_envelope),
    ]
}

#[test]
fn cgb_envelope_boundaries_are_accepted_by_the_constructor_and_decode() {
    for (_, build) in cgb_voice_kinds_with_envelope() {
        for envelope in [CGB_ENVELOPE_MIN, CGB_ENVELOPE_MAX] {
            let group = VoiceGroup::new(vec![build(envelope)]).unwrap();
            assert_eq!(VoiceGroup::decode(&group.encode()).unwrap(), group);
        }
    }
}

/// The constructor rejects each field independently, one past its mask's
/// domain, for every CGB voice kind.
#[test]
fn cgb_envelope_out_of_range_is_rejected_at_construction() {
    for (voice_kind, build) in cgb_voice_kinds_with_envelope() {
        let attack_too_high = Envelope {
            attack: 8,
            ..CGB_ENVELOPE_MIN
        };
        assert_eq!(
            VoiceGroup::new(vec![build(attack_too_high)]).unwrap_err(),
            AudioError::CgbEnvelopeOutOfRange {
                voice_kind,
                field: "attack",
                value: 8,
                maximum: 7,
            }
        );

        let decay_too_high = Envelope {
            decay: 8,
            ..CGB_ENVELOPE_MIN
        };
        assert_eq!(
            VoiceGroup::new(vec![build(decay_too_high)]).unwrap_err(),
            AudioError::CgbEnvelopeOutOfRange {
                voice_kind,
                field: "decay",
                value: 8,
                maximum: 7,
            }
        );

        let sustain_too_high = Envelope {
            sustain: 16,
            ..CGB_ENVELOPE_MIN
        };
        assert_eq!(
            VoiceGroup::new(vec![build(sustain_too_high)]).unwrap_err(),
            AudioError::CgbEnvelopeOutOfRange {
                voice_kind,
                field: "sustain",
                value: 16,
                maximum: 15,
            }
        );

        let release_too_high = Envelope {
            release: 8,
            ..CGB_ENVELOPE_MIN
        };
        assert_eq!(
            VoiceGroup::new(vec![build(release_too_high)]).unwrap_err(),
            AudioError::CgbEnvelopeOutOfRange {
                voice_kind,
                field: "release",
                value: 8,
                maximum: 7,
            }
        );
    }
}

/// Every CGB voice slot's wire layout ends with its four envelope bytes
/// (attack, decay, sustain, release) immediately followed by the trailing
/// `fixed_rate` byte, so a single-slot group's last five bytes locate the
/// envelope regardless of that voice kind's other variable-length fields
/// (e.g. `ProgrammableWave`'s wave id).
#[test]
fn decode_rejects_a_cgb_envelope_field_mutated_out_of_range() {
    let fields = [
        (0usize, "attack", MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE, 8u8),
        (1, "decay", MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE, 8),
        (2, "sustain", MAX_CGB_ENVELOPE_SUSTAIN, 16),
        (3, "release", MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE, 8),
    ];
    for (voice_kind, build) in cgb_voice_kinds_with_envelope() {
        for (field_offset, field, maximum, invalid_value) in fields {
            let group = VoiceGroup::new(vec![build(CGB_ENVELOPE_MIN)]).unwrap();
            let mut bytes = group.encode();
            let envelope_start = bytes.len() - 5;
            bytes[envelope_start + field_offset] = invalid_value;
            assert_eq!(
                VoiceGroup::decode(&bytes),
                Err(AudioError::CgbEnvelopeOutOfRange {
                    voice_kind,
                    field,
                    value: invalid_value,
                    maximum,
                }),
                "{voice_kind} {field} mutated to {invalid_value}"
            );
        }
    }
}
