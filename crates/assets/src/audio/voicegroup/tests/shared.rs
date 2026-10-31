use super::super::*;

pub(super) const MIDDLE_C: u8 = 60;
const KEY_SPLIT_START: u8 = 36;

pub(super) fn sample_envelope() -> Envelope {
    Envelope {
        attack: u8::MAX,
        decay: 0,
        sustain: u8::MAX,
        release: 0,
    }
}

/// Unlike `DirectSound`'s, a CGB envelope must survive upstream's macro masks unchanged
/// (`asm/macros/music_voice.inc`).
pub(super) fn sample_cgb_envelope() -> Envelope {
    Envelope {
        attack: MAX_CGB_ENVELOPE_ATTACK_DECAY_RELEASE,
        decay: 5,
        sustain: MAX_CGB_ENVELOPE_SUSTAIN,
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
