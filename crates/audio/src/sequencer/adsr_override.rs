//! Track-local ADSR overrides from the `xATTA`/`xDECA`/`xSUST`/`xRELE`
//! `XCMD`s.
//!
//! Upstream `ply_voice` copies the selected tone into `track->tone`
//! (`m4a_1.s:945`..`:965`) and each of `ply_xatta`, `ply_xdeca`, `ply_xsust`
//! and `ply_xrele` stores its one operand byte into that copy
//! (`m4a.c:1567`..`:1589`; dispatch `m4a_tables.c:297`..`:300`). `ply_note`
//! then snapshots the copy's ADSR into the new channel for a direct tone
//! (`m4a_1.s:1750`..`:1758`). The bank stays immutable here, so the copy is
//! modelled as the four overridden bytes, applied over the bank tone's ADSR
//! when a direct tone starts a note.

use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;

/// `XCMD` sub-command number for `xATTA` (`m4a_tables.c:297`).
pub(super) const XCMD_ATTA: u8 = 0x04;
/// `XCMD` sub-command number for `xDECA` (`m4a_tables.c:298`).
pub(super) const XCMD_DECA: u8 = 0x05;
/// `XCMD` sub-command number for `xSUST` (`m4a_tables.c:299`).
pub(super) const XCMD_SUST: u8 = 0x06;
/// `XCMD` sub-command number for `xRELE` (`m4a_tables.c:300`).
pub(super) const XCMD_RELE: u8 = 0x07;

/// The ADSR bytes a track has overwritten in its local tone copy since the
/// last `VOICE`; `None` keeps the bank tone's byte.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AdsrOverride {
    attack: Option<u8>,
    decay: Option<u8>,
    sustain: Option<u8>,
    release: Option<u8>,
}

impl AdsrOverride {
    /// Stores `value` into the field `kind` names (`ply_xatta`..`ply_xrele`);
    /// any other kind leaves the copy untouched.
    pub(super) fn set(&mut self, kind: u8, value: u8) {
        match kind {
            XCMD_ATTA => self.attack = Some(value),
            XCMD_DECA => self.decay = Some(value),
            XCMD_SUST => self.sustain = Some(value),
            XCMD_RELE => self.release = Some(value),
            _ => {}
        }
    }

    /// A DirectSound tone's ADSR with this track's overrides applied.
    pub(super) fn direct(self, adsr: Adsr) -> Adsr {
        Adsr {
            attack: self.attack.unwrap_or(adsr.attack),
            decay: self.decay.unwrap_or(adsr.decay),
            sustain: self.sustain.unwrap_or(adsr.sustain),
            release: self.release.unwrap_or(adsr.release),
        }
    }

    /// A CGB tone's ADSR with this track's overrides applied; the copy is the
    /// same four bytes (`m4a_1.s:1750`..`:1751`).
    pub(super) fn cgb(self, adsr: CgbAdsr) -> CgbAdsr {
        CgbAdsr {
            attack: self.attack.unwrap_or(adsr.attack),
            decay: self.decay.unwrap_or(adsr.decay),
            sustain: self.sustain.unwrap_or(adsr.sustain),
            release: self.release.unwrap_or(adsr.release),
        }
    }
}
