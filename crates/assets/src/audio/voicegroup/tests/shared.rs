use super::super::*;

pub(super) const MIDDLE_C: u8 = 60;
const KEY_SPLIT_START: u8 = 36;

pub(super) fn sample_envelope() -> Envelope {
    Envelope {
        attack: 255,
        decay: 0,
        sustain: 255,
        release: 0,
    }
}

/// A CGB envelope within upstream's macro-masked domain (`0..=7` for
/// attack/decay/release, `0..=15` for sustain). CGB voice fixtures use this
/// instead of [`sample_envelope`], whose out-of-domain values are valid only
/// for `DirectSound`'s unmasked envelope.
pub(super) fn sample_cgb_envelope() -> Envelope {
    Envelope {
        attack: 7,
        decay: 5,
        sustain: 15,
        release: 3,
    }
}

pub(super) fn all_voice_kinds() -> Vec<VoiceEntry> {
    vec![
        VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: MIDDLE_C,
            pan: Some(100),
            sample: SampleId("audio/sample/sc88pro_flute".to_owned()),
            envelope: sample_envelope(),
            mode: DirectSoundMode::Resampled,
        }),
        VoiceEntry::Square1(Square1Voice {
            base_key: MIDDLE_C,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: sample_cgb_envelope(),
            fixed_rate: false,
        }),
        VoiceEntry::Square2(Square2Voice {
            base_key: MIDDLE_C,
            length: 4,
            duty: 3,
            envelope: sample_cgb_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: MIDDLE_C,
            length: 0,
            wave: SampleId("audio/sample/programmable_wave_2".to_owned()),
            envelope: sample_cgb_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::Noise(NoiseVoice {
            base_key: MIDDLE_C,
            length: 0,
            period: 1,
            envelope: sample_cgb_envelope(),
            fixed_rate: false,
        }),
        VoiceEntry::KeySplit(
            KeySplitVoice::new(
                KEY_SPLIT_START,
                vec![0, 0, 1, 1, 2],
                VoiceGroupId("audio/voicegroup/trumpet_keysplit".to_owned()),
            )
            .unwrap(),
        ),
        VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId("audio/voicegroup/emerald_drumset_1".to_owned()),
        }),
        VoiceEntry::Empty,
    ]
}
