//! One playing CGB PSG voice: an oscillator ([`crate::psg`]) shaped by a
//! [`CgbEnvelope`] and routed to the stereo accumulator, mirroring
//! `CgbSound`'s per-hardware-channel loop (`m4a.c:925`) and its
//! `CgbPan`/`CgbModVol` panning helpers (`m4a.c:878`..`:923`).

use crate::cgb_envelope::{cgb_envelope_goal, cgb_pan, CgbAdsr, CgbEnvelope, Panning};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::{channel_volume, pan_terms, StereoAcc};

/// Which of the four fixed CGB hardware channels a voice occupies.
///
/// Unlike DirectSound's pooled voices, each of these exists exactly once —
/// starting a new note on the same channel number may replace whatever was
/// already sounding there, subject to `Mixer::add_cgb_voice`'s priority
/// test, mirroring `CgbSound`'s `for (ch = 1; ch <= 4; ch++)` loop over
/// fixed channel slots (`m4a.c:946`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CgbChannelNumber {
    Square1,
    Square2,
    Wave,
    Noise,
}

impl CgbChannelNumber {
    /// Index into a 4-slot array (`0..=3`).
    #[must_use]
    pub fn slot(self) -> usize {
        match self {
            Self::Square1 => 0,
            Self::Square2 => 1,
            Self::Wave => 2,
            Self::Noise => 3,
        }
    }
}

/// The waveform generator backing a live [`CgbVoice`].
#[derive(Clone, Debug)]
enum Oscillator {
    Square(SquareChannel),
    Wave(WaveChannel),
    Noise(NoiseChannel),
}

/// Common amplitude normalisation so every oscillator kind contributes a
/// comparable pre-envelope magnitude, in the same rough `s8` range
/// [`crate::voice::Voice`]'s interpolated samples occupy. Square/noise
/// oscillators are unit bipolar (`crate::psg`'s `-1`/`1`); the wave
/// channel's decoded nibble range (`-8..=7`) is scaled up to match.
fn oscillator_sample(osc: &mut Oscillator) -> i32 {
    match osc {
        Oscillator::Square(s) => i32::from(s.sample()) * 127,
        Oscillator::Wave(w) => i32::from(w.sample()) * 16,
        Oscillator::Noise(n) => i32::from(n.sample()) * 127,
    }
}

/// Channel-3 output level for a `0..=15` envelope volume, returned as a
/// `0..=256` gain numerator (100% -> 256, so the render path's `>> 8` yields
/// the fractional level).
///
/// Behavioural port of `gCgb3Vol[envelopeVolume]` (`m4a_tables.c:168`), the
/// table the driver writes to `NR32` for channel 3 (`m4a.c:1211`). Unlike the
/// square/noise channels — whose `NRx2` envelope volume scales the DAC roughly
/// linearly, modelled by the `* 8` in [`CgbVoice::begin_frame`] — channel 3
/// has only five coarse output levels, so its envelope is *quantised* here.
/// The `gCgb3Vol` codes decode to these `NR32` output-level percentages (bits:
/// `0x00` mute, `0x20` 100%, `0x40` 50%, `0x60` 25%, `0x80` the GBA's extra
/// 75%): envelope volume `0,1 -> 0%`, `2..=5 -> 25%`, `6..=9 -> 50%`,
/// `10..=13 -> 75%`, `14,15 -> 100%`. A centred note's envelope goal can reach
/// 31 (`cgb_envelope_goal`), past the 16-entry table; those saturate at 100%
/// here, since `gCgb3Vol` has no entry there.
fn cgb3_wave_level(envelope_volume: u8) -> u32 {
    const LEVEL_256: [u32; 16] = [
        0, 0, 64, 64, 64, 64, 128, 128, 128, 128, 192, 192, 192, 192, 256, 256,
    ];
    LEVEL_256[(envelope_volume as usize).min(15)]
}

/// The `NR43`-style noise control byte for `note_key`, with the instrument's
/// width selector folded into bit 3 (`0x08`).
///
/// The `gNoiseTable` byte carries clock-shift and divisor bits but never the
/// width bit (`m4a_tables.c:149`); `CgbSound` supplies it separately from the
/// instrument via `*nrx3ptr = wavePointer << 3` (`m4a.c:1022`), whose low bit
/// is `voice_noise`'s `period & 1` (`music_voice.inc:105`). Reproduced here by
/// ORing that bit into the table byte.
fn noise_control_byte(note_key: u8, lfsr_width_selector: u8) -> u8 {
    midi_key_to_noise_control(note_key) | ((lfsr_width_selector & 1) << 3)
}

/// Emerald's 8-bit-DAC frequency correction for a fixed-rate
/// (`TONEDATA_TYPE_FIX`) square/wave channel: `(freq_reg + 1) & 0x7fe`,
/// applied at note-on (before the oscillator — and, on channel 1, its sweep
/// unit's shadow frequency — initialize) and again on every mid-note pitch
/// re-run (`m4a.c:1184`..`:1202`, under the 8-bit DAC configuration selected
/// at `m4a.c:70`..`:81`). A non-fixed-rate channel's register passes through
/// unmodified.
fn cgb_dac_correct(freq_reg: u16, fixed_rate: bool) -> u16 {
    if fixed_rate {
        (freq_reg + 1) & 0x7fe
    } else {
        freq_reg
    }
}

/// A live CGB PSG voice.
#[derive(Clone, Debug)]
pub struct CgbVoice {
    channel: CgbChannelNumber,
    oscillator: Oscillator,
    envelope: CgbEnvelope,
    adsr: CgbAdsr,
    base_right: u8,
    base_left: u8,
    velocity: u8,
    left_enabled: bool,
    right_enabled: bool,
    env_gain: i32,
    gate_time: u16,
    /// The played key, for tie/end-of-tie matching (always the played key,
    /// even for a rhythm indirection — see [`crate::voice::Voice::midi_key`]).
    midi_key: u8,
    /// The key fed to `MidiKeyToCgbFreq` on a mid-note pitch re-run. Equal to
    /// [`Self::midi_key`] for a plain or key-split note; a rhythm
    /// indirection substitutes its child's own base key instead
    /// (`ply_note`, `m4a_1.s:1594`).
    pitch_key: u8,
    /// `ChnVolSetAsm`'s rhythm-pan override, folded into every
    /// [`Self::set_track_volume`] re-run; `0` outside rhythm.
    rhythm_pan: i8,
    track: usize,
    /// Monotonic note-on ordinal, shared with the DirectSound
    /// [`crate::voice::Voice`]s so an end-of-tie can pick the newest match
    /// across both kinds (see that type's `seq` field). Stamped by the mixer.
    seq: u64,
    /// `TONEDATA_TYPE_FIX`, threaded from the instrument at note-on: whether
    /// [`Self::set_track_pitch`] re-applies [`cgb_dac_correct`] on every
    /// mid-note retune. Always `false` for a noise voice (see [`Self::noise`]).
    fixed_rate: bool,
    /// The note's effective priority (`CgbChannel::priority`) -- see
    /// [`crate::voice::Voice::priority`]. `ply_note`'s CGB arm compares it
    /// against the occupant of this voice's one fixed hardware channel
    /// before overwriting it (`m4a_1.s:1647`..`:1668`).
    priority: u8,
}

impl CgbVoice {
    /// Start a square-channel (1 or 2) voice, not fixed-rate. `sweep_byte` is
    /// `Some` only for channel 1 (channel 2 has no hardware sweep register to
    /// drive). See [`Self::square_with_fixed_rate`] for a `TONEDATA_TYPE_FIX`
    /// instrument.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn square(
        channel: CgbChannelNumber,
        duty: u8,
        sweep_byte: Option<u8>,
        adsr: CgbAdsr,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        Self::square_with_fixed_rate(
            channel,
            duty,
            sweep_byte,
            adsr,
            false,
            note_key,
            pit_m,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    /// As [`Self::square`], additionally threading the instrument's
    /// `TONEDATA_TYPE_FIX` flag through to Emerald's 8-bit-DAC frequency
    /// correction ([`cgb_dac_correct`]) before the oscillator — and, for
    /// channel 1, its sweep unit's shadow frequency — initialize
    /// (`m4a.c:1184`..`:1202`), and to every later [`Self::set_track_pitch`]
    /// re-run.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn square_with_fixed_rate(
        channel: CgbChannelNumber,
        duty: u8,
        sweep_byte: Option<u8>,
        adsr: CgbAdsr,
        fixed_rate: bool,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let freq_reg = cgb_dac_correct(midi_key_to_cgb_freq_reg(note_key, pit_m), fixed_rate);
        let sweep = sweep_byte.map(|b| crate::psg::Sweep::from_byte(b, freq_reg));
        let square = SquareChannel::new(duty, freq_reg, sweep);
        // Hardware runs the sweep overflow calc immediately at trigger, so an
        // upward sweep whose first step overflows disables the channel at
        // note-on before any tick (`mgba audio.c:184`). Mirror the tick-time
        // sweep-disable path (see [`Self::begin_frame`]): retire the envelope so
        // the voice is inactive from frame 0.
        let born_dead = square.is_disabled();
        let oscillator = Oscillator::Square(square);
        let mut voice = Self::new(
            channel,
            oscillator,
            adsr,
            fixed_rate,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        );
        if born_dead {
            voice.envelope.retire();
        }
        voice
    }

    /// Start a programmable-wave (channel 3) voice from already-decoded
    /// samples (see [`crate::psg::WaveChannel::decode_wave_ram`]).
    /// `fixed_rate` threads the instrument's `TONEDATA_TYPE_FIX` flag through
    /// to Emerald's 8-bit-DAC frequency correction ([`cgb_dac_correct`])
    /// before the oscillator initializes, and to every later
    /// [`Self::set_track_pitch`] re-run.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn wave(
        samples: [i8; 32],
        adsr: CgbAdsr,
        fixed_rate: bool,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let freq_reg = cgb_dac_correct(midi_key_to_cgb_freq_reg(note_key, pit_m), fixed_rate);
        let oscillator = Oscillator::Wave(WaveChannel::new(samples, freq_reg));
        Self::new(
            CgbChannelNumber::Wave,
            oscillator,
            adsr,
            fixed_rate,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    /// Start a noise (channel 4) voice. Noise ignores fine pitch, matching
    /// `MidiKeyToCgbFreq`'s noise branch (`m4a.c:812`), and is never
    /// fixed-rate (Emerald's 8-bit-DAC correction applies only to the
    /// pitched square/wave channels — `m4a.c:1184`..`:1202`).
    /// `lfsr_width_selector` is the instrument's width selector (`ToneData`
    /// byte from `voice_noise`'s `period & 1`, `music_voice.inc:105`): its
    /// low bit becomes `NR43` bit 3 (`0x08`), which `CgbSound` sets via
    /// `*nrx3ptr = wavePointer << 3` (`m4a.c:1022`) and which the
    /// `gNoiseTable` control byte never carries itself — so it alone selects
    /// the LFSR's narrow (7-bit) mode.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn noise(
        adsr: CgbAdsr,
        note_key: u8,
        lfsr_width_selector: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let control = noise_control_byte(note_key, lfsr_width_selector);
        let oscillator = Oscillator::Noise(NoiseChannel::from_control_byte(control));
        Self::new(
            CgbChannelNumber::Noise,
            oscillator,
            adsr,
            false,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        channel: CgbChannelNumber,
        oscillator: Oscillator,
        adsr: CgbAdsr,
        fixed_rate: bool,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let (pan_right, pan_left) = pan_terms(rhythm_pan);
        let base_right = channel_volume(vol_mr, pan_right, velocity);
        let base_left = channel_volume(vol_ml, pan_left, velocity);
        let panning = cgb_pan(base_right, base_left);
        let goal = cgb_envelope_goal(base_right, base_left, panning);
        Self {
            channel,
            oscillator,
            envelope: CgbEnvelope::new(adsr, goal, echo_volume, echo_length),
            adsr,
            base_right,
            base_left,
            velocity,
            left_enabled: matches!(panning, Panning::Left | Panning::Both),
            right_enabled: matches!(panning, Panning::Right | Panning::Both),
            env_gain: 0,
            gate_time,
            midi_key,
            pitch_key: midi_key,
            rhythm_pan,
            track,
            seq: 0,
            fixed_rate,
            priority: 0,
        }
    }

    /// Stamp this voice's effective note-on priority (see [`Self::priority`]).
    #[must_use]
    pub(crate) fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// This voice's effective note-on priority (higher outranks lower).
    #[must_use]
    pub(crate) fn priority(&self) -> u8 {
        self.priority
    }

    /// Override the key used for pitch resolution independently of
    /// [`Self::midi_key`] — a rhythm indirection's child base key
    /// (`ply_note`, `m4a_1.s:1594`). Chained onto a constructor so ordinary
    /// (non-rhythm) notes need not thread an extra parameter.
    #[must_use]
    pub(crate) fn with_pitch_key(mut self, pitch_key: u8) -> Self {
        self.pitch_key = pitch_key;
        self
    }

    /// Stamp this voice's shared note-on ordinal (see [`Self::seq`]). Called by
    /// the mixer as it accepts the voice.
    pub(crate) fn set_seq(&mut self, seq: u64) {
        self.seq = seq;
    }

    /// This voice's shared note-on ordinal (higher is newer).
    #[must_use]
    pub(crate) fn seq(&self) -> u64 {
        self.seq
    }

    /// Which hardware channel slot this voice occupies.
    #[must_use]
    pub fn channel(&self) -> CgbChannelNumber {
        self.channel
    }

    /// The owning track index.
    #[must_use]
    pub fn track(&self) -> usize {
        self.track
    }

    /// The MIDI key this voice was started on.
    #[must_use]
    pub fn midi_key(&self) -> u8 {
        self.midi_key
    }

    /// Whether the voice is still producing sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Whether the note has already been released.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.envelope.is_stopping()
    }

    /// Tick the note-off gate down by one sequencer tick.
    pub fn tick_gate(&mut self) {
        if self.gate_time > 0 {
            self.gate_time -= 1;
            if self.gate_time == 0 {
                self.envelope.note_off();
            }
        }
    }

    /// Force note-off (tie termination / explicit stop).
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Re-run `ChnVolSetAsm` + `CgbModVol` against this live channel from
    /// updated track `volMR`/`volML`: rewrites the base volumes, panning,
    /// and envelope goal (`MPT_FLG_VOLCHG`, `m4a_1.s:1394`..`:1401`).
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        let (pan_right, pan_left) = pan_terms(self.rhythm_pan);
        self.base_right = channel_volume(vol_mr, pan_right, self.velocity);
        self.base_left = channel_volume(vol_ml, pan_left, self.velocity);
        let panning = cgb_pan(self.base_right, self.base_left);
        self.left_enabled = matches!(panning, Panning::Left | Panning::Both);
        self.right_enabled = matches!(panning, Panning::Right | Panning::Both);
        let goal = cgb_envelope_goal(self.base_right, self.base_left, panning);
        self.envelope.set_goal(self.adsr, goal);
    }

    /// Recompute this live channel's playback frequency from updated track
    /// `keyM`/`pitM`, mirroring `MidiKeyToCgbFreq` re-runs in `MPlayMain`'s
    /// per-tick pitch pass (`m4a_1.s:1416`..`:1425`). Noise channels ignore
    /// `pit_m`, matching `MidiKeyToCgbFreq`'s noise branch.
    ///
    /// The noise retune only carries the table's clock/divisor bits; the width
    /// selector set at note-on is preserved by [`NoiseChannel::retune`],
    /// mirroring `CgbSound`'s `*nrx3ptr = (*nrx3ptr & 0x08) | frequency`
    /// (`m4a.c:1200`).
    ///
    /// A fixed-rate square/wave channel (`Self::fixed_rate`) re-applies
    /// [`cgb_dac_correct`] here too, so a mid-note pitch re-run can't undo the
    /// correction note-on applied.
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let note_key = u8::try_from((i32::from(self.pitch_key) + key_m).max(0) & 0xFF).unwrap_or(0);
        match &mut self.oscillator {
            Oscillator::Square(s) => s.set_frequency(cgb_dac_correct(
                midi_key_to_cgb_freq_reg(note_key, pit_m),
                self.fixed_rate,
            )),
            Oscillator::Wave(w) => w.set_frequency(cgb_dac_correct(
                midi_key_to_cgb_freq_reg(note_key, pit_m),
                self.fixed_rate,
            )),
            Oscillator::Noise(n) => n.retune(midi_key_to_noise_control(note_key)),
        }
    }

    /// Advance the envelope one frame and recompute the frame's gain.
    /// Channel-1 sweep steps are *not* driven from here: unlike the
    /// once-per-buffer M4A software envelope, the sweep unit ticks at
    /// hardware's own 128 Hz cadence, which does not line up with render
    /// buffer boundaries — see [`Self::render`]'s `sweep_ticks` parameter
    /// and [`crate::psg::FrameSequencer128Hz`] (issue #381).
    pub fn begin_frame(&mut self, master_volume: u8) {
        self.envelope.step();
        // Scale the envelope's coarse level up to the same rough `0..=255`
        // range `Voice::begin_frame` mixes at, so a CGB channel's loudness is
        // comparable to a DirectSound one at the same nominal volume. The wave
        // channel takes its level from `gCgb3Vol`'s stepped quantisation
        // (`m4a.c:1211`) instead of the square/noise linear `* 8`, since
        // channel-3 amplitude comes solely from that table (see
        // [`cgb3_wave_level`]).
        let volume_255 = match &self.oscillator {
            Oscillator::Wave(_) => cgb3_wave_level(self.envelope.volume()),
            _ => u32::from(self.envelope.volume()) * 8,
        };
        let effective = ((u32::from(master_volume) + 1) * volume_255) >> 4;
        self.env_gain = i32::try_from(effective).unwrap_or(i32::MAX);
    }

    /// Render this voice's contribution across a frame, accumulating into
    /// `acc`. [`Self::begin_frame`] must have run for this frame first.
    ///
    /// `sweep_ticks` are the ascending, 0-based sample offsets within `acc`
    /// at which the channel-1 sweep must tick — normally
    /// [`crate::psg::FrameSequencer128Hz::advance`]'s result for `acc.len()`
    /// samples, shared across every CGB voice in a frame since the real
    /// frame sequencer is one clock for the whole hardware unit. Applying
    /// the tick at its exact sample offset (rather than once before the
    /// whole buffer) is what gives a sweeping channel-1 voice hardware's
    /// 128 Hz cadence instead of the render buffer's ~59.73 Hz (issue
    /// #381). Ignored by non-square oscillators and by a square channel
    /// with no sweep configured.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        let mut ticks = sweep_ticks.iter().copied().peekable();
        for (i, slot) in acc.iter_mut().enumerate() {
            if !self.envelope.is_active() {
                break;
            }
            if ticks.peek() == Some(&i) {
                ticks.next();
                if let Oscillator::Square(s) = &mut self.oscillator {
                    if !s.step_sweep_tick() {
                        // Hardware silences the channel from this exact tick
                        // onward, so no further samples of this buffer may
                        // render — mirroring how a trigger-time overflow
                        // leaves a voice silent from frame 0
                        // (`Self::square_with_fixed_rate`'s `born_dead`).
                        self.envelope.retire();
                        break;
                    }
                }
            }
            let raw = oscillator_sample(&mut self.oscillator);
            let contribution = (self.env_gain * raw) >> 8;
            if self.right_enabled {
                slot.1 += contribution;
            }
            if self.left_enabled {
                slot.0 += contribution;
            }
        }
    }
}

#[cfg(test)]
impl CgbVoice {
    /// Whether this voice's noise oscillator is in narrow (7-bit) mode, or
    /// `None` if it is not a noise voice.
    fn noise_is_narrow(&self) -> Option<bool> {
        match &self.oscillator {
            Oscillator::Noise(n) => Some(n.is_narrow()),
            _ => None,
        }
    }

    /// This voice's channel-1 sweep shadow frequency register, or `None` if
    /// it is not a square voice with a sweep configured. Test-only
    /// introspection for pinning sweep tick cadence (issue #381).
    fn sweep_frequency(&self) -> Option<u16> {
        match &self.oscillator {
            Oscillator::Square(s) => s.sweep_frequency(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psg::FrameSequencer128Hz;

    #[test]
    fn noise_control_byte_sets_width_bit_only_for_odd_period() {
        // The `gNoiseTable` entry itself never carries the width bit
        // (`m4a_tables.c:149`); the instrument's `period` supplies it.
        assert_eq!(midi_key_to_noise_control(60) & 0x08, 0);
        assert_eq!(noise_control_byte(60, 1) & 0x08, 0x08);
        assert_eq!(noise_control_byte(60, 0) & 0x08, 0);
    }

    #[test]
    fn noise_period_bit_drives_narrow_lfsr_mode() {
        // A `period == 1` instrument must produce a narrow (periodic) noise
        // channel; `period == 0` stays wide. Before the fix `NoiseTone`
        // carried no period, so narrow mode was unreachable and every
        // instrument played wide 15-bit noise.
        let narrow = CgbVoice::noise(CgbAdsr::flat(), 60, 1, 0xFF, 0xFF, 127, 0, 60, 0, 0, 0, 0);
        assert_eq!(narrow.noise_is_narrow(), Some(true));
        let wide = CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0xFF, 0xFF, 127, 0, 60, 0, 0, 0, 0);
        assert_eq!(wide.noise_is_narrow(), Some(false));
    }

    #[test]
    fn noise_retune_preserves_the_width_bit() {
        // `MidiKeyToCgbFreq`'s noise retune supplies only clock/divisor bits;
        // the width selector set at note-on survives (`m4a.c:1200`).
        let mut narrow =
            CgbVoice::noise(CgbAdsr::flat(), 60, 1, 0xFF, 0xFF, 127, 0, 60, 0, 0, 0, 0);
        narrow.set_track_pitch(12, 0);
        assert_eq!(narrow.noise_is_narrow(), Some(true));
    }

    /// A full-swing wave table (nibbles alternating 0x0 and 0xF -> decoded
    /// -8 / 7), so any non-zero output level produces audible samples.
    fn full_swing_wave() -> [i8; 32] {
        WaveChannel::decode_wave_ram(&[0x0F; 16])
    }

    #[test]
    fn wave_note_with_active_envelope_is_audible() {
        // Channel-3 amplitude comes solely from the envelope via `gCgb3Vol`
        // (`m4a.c:1211`); with a live (flat, full-volume) envelope the note must
        // produce sound. Before the fix a loader-defaulted `volume_shift` of 0
        // muted the generator outright, silencing every wave note — this test
        // would fail under that bug.
        let mut voice = CgbVoice::wave(
            full_swing_wave(),
            CgbAdsr::flat(),
            false,
            60,
            0,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
            0,
        );
        let mut acc = vec![(0i32, 0i32); 8];
        voice.begin_frame(15);
        voice.render(&mut acc, &[]);
        assert!(
            acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "a live-envelope wave note must be audible"
        );
    }

    #[test]
    fn cgb3_wave_level_pins_the_stepped_output_levels() {
        // `gCgb3Vol` quantises the envelope to five NR32 levels
        // (`m4a_tables.c:168`): 0,1 -> mute; 2..=5 -> 25%; 6..=9 -> 50%;
        // 10..=13 -> 75%; 14,15 -> 100%. Returned as a 0..=256 numerator.
        assert_eq!(cgb3_wave_level(0), 0); // mute
        assert_eq!(cgb3_wave_level(1), 0); // mute
        assert_eq!(cgb3_wave_level(2), 64); // 25%
        assert_eq!(cgb3_wave_level(6), 128); // 50%
        assert_eq!(cgb3_wave_level(10), 192); // 75%
        assert_eq!(cgb3_wave_level(15), 256); // 100%
                                              // A centred note's goal can exceed 15; those saturate at 100%.
        assert_eq!(cgb3_wave_level(31), 256);
    }

    #[test]
    fn square1_upward_sweep_overflow_is_born_dead() {
        // A very high note (key 120 -> frequency register near 0x7FF) with an
        // upward sweep (shift 1) overflows the trigger-time overflow calc, so
        // the channel is inactive and silent from frame 0 — including with a
        // sweep period of 0 (`mgba audio.c:184`). A normal note is unaffected.
        for sweep_byte in [0b0000_0001u8, 0b0011_0001u8] {
            // period 0 and period 3, both add + shift 1
            let mut dead = CgbVoice::square(
                CgbChannelNumber::Square1,
                2,
                Some(sweep_byte),
                CgbAdsr::flat(),
                120,
                0,
                0xFF,
                0xFF,
                127,
                0,
                120,
                0,
                0,
                0,
                0,
            );
            assert!(!dead.is_active(), "overflowing sweep note is born dead");
            let mut acc = vec![(0i32, 0i32); 8];
            dead.begin_frame(15);
            dead.render(&mut acc, &[]);
            assert!(
                acc.iter().all(|&(l, r)| l == 0 && r == 0),
                "born-dead channel must be silent from frame 0"
            );
        }

        let normal = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            Some(0b0000_0001), // period 0, add, shift 1
            CgbAdsr::flat(),
            48, // low note: frequency register well within 0x7FF
            0,
            0xFF,
            0xFF,
            127,
            0,
            48,
            0,
            0,
            0,
            0,
        );
        assert!(
            normal.is_active(),
            "a normal-frequency sweep note keeps playing"
        );
    }

    #[test]
    fn with_pitch_key_overrides_the_base_used_for_mid_note_bend() {
        // A rhythm child's own base key (72) diverges from the played key
        // (60) used for tie matching; a mid-note pitch re-run must recompute
        // from the CHILD's base key, not the played one.
        assert_eq!(
            CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0xFF, 0xFF, 127, 0, 60, 0, 0, 0, 0)
                .with_pitch_key(72)
                .midi_key(),
            60,
            "EOT identity stays the played key"
        );

        // `set_track_pitch(0, 0)` re-derives the oscillator's frequency from
        // `pitch_key` (72) rather than `midi_key` (60); a channel constructed
        // straight at key 72 must render identically once re-pitched.
        let mut square = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            None,
            CgbAdsr::flat(),
            60,
            0,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            0,
            0,
        )
        .with_pitch_key(72);
        square.set_track_pitch(0, 0);
        let mut expected = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            None,
            CgbAdsr::flat(),
            72,
            0,
            0xFF,
            0xFF,
            127,
            0,
            72,
            0,
            0,
            0,
            0,
        );
        let mut acc_a = vec![(0i32, 0i32); 16];
        let mut acc_b = vec![(0i32, 0i32); 16];
        square.begin_frame(15);
        square.render(&mut acc_a, &[]);
        expected.begin_frame(15);
        expected.render(&mut acc_b, &[]);
        assert_eq!(acc_a, acc_b);
    }

    #[test]
    fn rhythm_pan_shifts_the_stereo_split_on_construction_and_reruns() {
        // A positive rhythm-pan override must favour the right channel over
        // an otherwise-centred track, both at note-on and after a VOL rerun.
        let centred = CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0x40, 0x40, 127, 0, 60, 0, 0, 0, 0);
        let panned = CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0x40, 0x40, 127, 0, 60, 0, 63, 0, 0);
        assert!(
            panned.base_right > centred.base_right,
            "a positive rhythm pan should raise the right-channel base volume"
        );

        let mut voice =
            CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0xFF, 0xFF, 127, 0, 60, 0, 63, 0, 0);
        voice.set_track_volume(0x40, 0x40);
        assert!(
            voice.base_right > voice.base_left,
            "the rhythm-pan override must survive a mid-note VOL rerun"
        );
    }

    #[test]
    fn echo_volume_and_length_are_copied_into_the_envelope_at_construction() {
        // A CGB voice's constructor must thread echo_volume/echo_length into
        // its CgbEnvelope so a release with no ADSR release period still
        // reaches the pseudo-echo tail instead of silencing immediately.
        let mut voice = CgbVoice::noise(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            60,
            0,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
            0,
            128,
            3,
        );
        voice.begin_frame(15);
        voice.note_off();
        voice.begin_frame(15);
        assert!(
            voice.is_active(),
            "a nonzero echo_volume must hold the channel in its pseudo-echo tail"
        );
    }

    #[test]
    fn cgb_dac_correct_matches_emeralds_8_bit_dac_formula() {
        // `(freq_reg + 1) & 0x7fe` (m4a.c:1184..:1202): a fixed-rate register
        // rounds up to the next even value; a non-fixed-rate one passes
        // through unmodified.
        assert_eq!(cgb_dac_correct(0, true), 0);
        assert_eq!(cgb_dac_correct(1, true), 2);
        assert_eq!(cgb_dac_correct(2, true), 2);
        assert_eq!(cgb_dac_correct(0x7FE, true), 0x7FE);
        assert_eq!(cgb_dac_correct(0x555, false), 0x555);
    }

    #[test]
    fn fixed_rate_note_on_applies_the_dac_correction_before_the_sweep_born_dead_check() {
        // Key 54/fine 167 lands on the odd frequency register 0x555 (1365):
        // its sweep-overflow sum (freq + freq>>shift) sits exactly at the
        // 0x7FF threshold, so a plain (non-fixed-rate) note is NOT born
        // dead. The DAC-corrected register 0x556 (1366) pushes that same sum
        // over the threshold, so a fixed-rate note on the identical
        // key/sweep IS born dead — which only holds if the correction runs
        // before `SquareChannel::new`'s sweep initialization, not after.
        let sweep_byte = 0b0000_0001; // period 0, add, shift 1
        let plain = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            Some(sweep_byte),
            CgbAdsr::flat(),
            54,
            167,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        );
        assert!(
            plain.is_active(),
            "the uncorrected sum sits exactly at the threshold, not over it"
        );

        let fixed = CgbVoice::square_with_fixed_rate(
            CgbChannelNumber::Square1,
            2,
            Some(sweep_byte),
            CgbAdsr::flat(),
            true,
            54,
            167,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        );
        assert!(
            !fixed.is_active(),
            "the DAC-corrected sum must overflow the sweep, born dead"
        );
    }

    #[test]
    fn fixed_rate_wave_note_on_audibly_differs_from_the_uncorrected_register() {
        // Key 54/fine 167 lands on the odd register 0x555; the DAC
        // correction rounds it up to 0x556, a different playback rate. Over
        // enough samples a one-register-step difference must show up in the
        // rendered waveform.
        let mut fixed = CgbVoice::wave(
            full_swing_wave(),
            CgbAdsr::flat(),
            true,
            54,
            167,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        );
        let mut plain = CgbVoice::wave(
            full_swing_wave(),
            CgbAdsr::flat(),
            false,
            54,
            167,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        );
        let mut acc_fixed = vec![(0i32, 0i32); 2048];
        let mut acc_plain = vec![(0i32, 0i32); 2048];
        fixed.begin_frame(15);
        fixed.render(&mut acc_fixed, &[]);
        plain.begin_frame(15);
        plain.render(&mut acc_plain, &[]);
        assert_ne!(
            acc_fixed, acc_plain,
            "the DAC-corrected register must audibly differ from the uncorrected one"
        );
    }

    #[test]
    fn set_track_pitch_reapplies_the_dac_correction_for_a_fixed_rate_channel() {
        // A fixed-rate voice built directly at key 54/fine 167 must render
        // identically to one built elsewhere then bent to that same key via
        // `set_track_pitch` — a mid-note retune must reapply the DAC
        // correction, not just the raw register.
        let mut direct = CgbVoice::square_with_fixed_rate(
            CgbChannelNumber::Square2,
            2,
            None,
            CgbAdsr::flat(),
            true,
            54,
            167,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        );
        let mut retuned = CgbVoice::square_with_fixed_rate(
            CgbChannelNumber::Square2,
            2,
            None,
            CgbAdsr::flat(),
            true,
            60,
            0,
            0xFF,
            0xFF,
            127,
            0,
            54,
            0,
            0,
            0,
            0,
        )
        .with_pitch_key(54);
        retuned.set_track_pitch(0, 167);

        let mut acc_direct = vec![(0i32, 0i32); 2048];
        let mut acc_retuned = vec![(0i32, 0i32); 2048];
        direct.begin_frame(15);
        direct.render(&mut acc_direct, &[]);
        retuned.begin_frame(15);
        retuned.render(&mut acc_retuned, &[]);
        assert_eq!(acc_direct, acc_retuned);
    }

    /// A square-1 voice at the lowest playable frequency register (key `0`
    /// clamps to `44`), leaving plenty of headroom before a shift-1 sweep's
    /// compounding `x1.5`-per-tick growth overflows `0x7FF` — verified
    /// empirically to stay in range through at least 7 successive period-1
    /// ticks. Used by the 128 Hz cadence and chunk-boundary tests below,
    /// which need several ticks without the channel disabling mid-test.
    fn low_freq_sweep_voice(sweep_byte: u8) -> CgbVoice {
        CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            Some(sweep_byte),
            CgbAdsr::flat(),
            0,
            0,
            0xFF,
            0xFF,
            127,
            0,
            0,
            0,
            0,
            0,
            0,
        )
    }

    /// Drive `voice` through every offset in `ticks` one sample at a time,
    /// recording [`CgbVoice::sweep_frequency`] immediately after each tick
    /// fires. Samples between ticks render with no ticks of their own.
    fn sweep_frequency_after_each_tick(voice: &mut CgbVoice, ticks: &[usize]) -> Vec<u16> {
        let mut cursor = 0usize;
        let mut freqs = Vec::with_capacity(ticks.len());
        for &tick in ticks {
            let gap = tick - cursor;
            if gap > 0 {
                voice.render(&mut vec![(0i32, 0i32); gap], &[]);
            }
            voice.render(&mut [(0i32, 0i32)], &[0]);
            freqs.push(
                voice
                    .sweep_frequency()
                    .expect("still a square voice with a sweep configured"),
            );
            cursor = tick + 1;
        }
        freqs
    }

    #[test]
    fn square1_sweep_period_1_ticks_at_every_128hz_sample_offset() {
        // period 1, add, shift 1: `GBAudioUpdateFrame`'s `case 2:`/`case 6:`
        // arm (`mgba/src/gb/audio.c:663`..`:668`) fires the sweep on every
        // 128 Hz tick when `period == 1`. Pin that the shadow frequency
        // changes at each of `FrameSequencer128Hz`'s tick offsets — not
        // once per ~59.73 Hz render buffer, as before this fix (issue
        // #381).
        let mut voice = low_freq_sweep_voice(0x11);
        voice.begin_frame(15);
        let start_freq = voice.sweep_frequency().expect("square1 sweep configured");

        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(600); // 5 ticks, well short of the 8th (overflow)
        let freqs = sweep_frequency_after_each_tick(&mut voice, &ticks);

        assert_eq!(freqs.len(), 5);
        let mut expected = start_freq;
        for &f in &freqs {
            assert!(
                f > expected,
                "a period-1 sweep must advance on every 128 Hz tick, got {freqs:?}"
            );
            expected = f;
        }
    }

    #[test]
    fn square1_sweep_period_2_ticks_at_half_the_128hz_rate() {
        // period 2 (the issue's real repro: `rs_sfx_1.inc`'s
        // `voice_square_1_alt 60, 0, 44, 2, 0, 4, 0, 0` and `..., 38, 0, ...`
        // both encode period 2): the sweep must fire on every SECOND 128 Hz
        // tick (64 Hz), not the ~29.86 Hz the old once-per-render-buffer
        // cadence produced.
        let mut voice = low_freq_sweep_voice(0x21); // period 2, add, shift 1
        voice.begin_frame(15);
        let start_freq = voice.sweep_frequency().expect("square1 sweep configured");

        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(1200); // 11 ticks
        let freqs = sweep_frequency_after_each_tick(&mut voice, &ticks);

        assert_eq!(freqs.len(), 11);
        let mut expected = start_freq;
        let mut should_fire = false; // period 2's counter starts at 2: the
                                     // first tick of each pair only counts
                                     // down, the second fires.
        for &f in &freqs {
            if should_fire {
                assert!(
                    f > expected,
                    "the second 128 Hz tick of a period-2 pair must fire, got {freqs:?}"
                );
                expected = f;
            } else {
                assert_eq!(
                    f, expected,
                    "the first 128 Hz tick of a period-2 pair must not fire, got {freqs:?}"
                );
            }
            should_fire = !should_fire;
        }
    }

    #[test]
    fn cgb_voice_render_is_chunk_boundary_invariant() {
        // Rendering the same stream as one buffer or as two half-size
        // buffers back to back must produce identical audio: the persistent
        // `FrameSequencer128Hz` clock and the voice's own oscillator/sweep
        // state are unaffected by how a caller chunks its render calls
        // (issue #381).
        let make_voice = || low_freq_sweep_voice(0x11); // period 1, add, shift 1

        let mut whole_voice = make_voice();
        whole_voice.begin_frame(15);
        let mut whole_clock = FrameSequencer128Hz::default();
        let whole_ticks = whole_clock.advance(600);
        let mut whole_acc = vec![(0i32, 0i32); 600];
        whole_voice.render(&mut whole_acc, &whole_ticks);

        let mut split_voice = make_voice();
        split_voice.begin_frame(15);
        let mut split_clock = FrameSequencer128Hz::default();
        let first_ticks = split_clock.advance(300);
        let mut first_half = vec![(0i32, 0i32); 300];
        split_voice.render(&mut first_half, &first_ticks);
        let second_ticks = split_clock.advance(300);
        let mut second_half = vec![(0i32, 0i32); 300];
        split_voice.render(&mut second_half, &second_ticks);
        let mut split_acc = first_half;
        split_acc.extend(second_half);

        assert_eq!(whole_acc, split_acc);
        assert!(
            whole_acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "sanity: the sweeping voice must actually be audible"
        );
    }

    #[test]
    fn square1_sweep_overflow_retires_the_voice_mid_buffer() {
        // Key 48 (frequency register 1046) with sweep byte 0x11 (period 1,
        // add, shift 1) survives construction (the trigger-time check alone
        // doesn't overflow, `psg.rs`'s `sweep_square_channel_and_voice_retire_
        // on_lookahead_overflow`), but its very first 128 Hz tick's
        // post-update look-ahead does (`_updateSweep(ch, true)`,
        // `mgba/src/gb/audio.c:980`..`:981`, ported as
        // `Sweep::tick`'s look-ahead branch). Pin that retirement lands at
        // that tick's exact sample offset within a render buffer, not only
        // at the buffer's end (issue #381).
        let mut voice = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            Some(0x11),
            CgbAdsr::flat(),
            48,
            0,
            0xFF,
            0xFF,
            127,
            0,
            48,
            0,
            0,
            0,
            0,
        );
        assert!(
            voice.is_active(),
            "not born dead: the trigger check alone doesn't overflow"
        );
        voice.begin_frame(15);

        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(300);
        let first_tick = ticks[0];
        let mut acc = vec![(0i32, 0i32); 300];
        voice.render(&mut acc, &ticks);

        assert!(
            acc[..first_tick].iter().any(|&(l, r)| l != 0 || r != 0),
            "samples before the overflowing tick must still be audible"
        );
        assert!(
            acc[first_tick..].iter().all(|&(l, r)| l == 0 && r == 0),
            "samples from the overflowing tick onward must be silent, not just \
             at the buffer end"
        );
        assert!(
            !voice.is_active(),
            "the voice must retire once the sweep overflows"
        );
    }
}
