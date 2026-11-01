use crate::cgb_envelope::{cgb_envelope_goal, cgb_pan, Panning};
use crate::voice::{channel_volume, pan_terms, StereoAcc};

#[derive(Clone, Copy, Debug)]
pub(super) struct StereoRouting {
    pub(super) right: u8,
    pub(super) left: u8,
    velocity: u8,
    rhythm_pan: i8,
    /// `chan->pan`: recomputed at every envelope boundary (`recompute_pan`'s
    /// doc), independent of whether it has reached the rendered route yet.
    calculated_pan: Panning,
    pub(super) right_enabled: bool,
    pub(super) left_enabled: bool,
}

impl StereoRouting {
    pub(super) fn new(track_right: u8, track_left: u8, velocity: u8, rhythm_pan: i8) -> Self {
        let mut routing = Self {
            right: 0,
            left: 0,
            velocity,
            rhythm_pan,
            calculated_pan: Panning::Right,
            right_enabled: false,
            left_enabled: false,
        };
        routing.update_volumes(track_right, track_left);
        routing.recompute_pan();
        routing.commit_pan();
        routing
    }

    /// Recompute the base side volumes only, matching `ChnVolSetAsm`, which
    /// leaves `chan->pan` untouched (`m4a_1.s:1508-1536`).
    pub(super) fn update_volumes(&mut self, track_right: u8, track_left: u8) {
        let (pan_right, pan_left) = pan_terms(self.rhythm_pan);
        self.right = channel_volume(track_right, pan_right, self.velocity);
        self.left = channel_volume(track_left, pan_left, self.velocity);
    }

    /// Recompute `chan->pan` from the latest side volumes, matching `CgbPan`
    /// (`m4a.c:878-901`); by itself this moves nothing audible (`commit_pan`'s doc).
    pub(super) fn recompute_pan(&mut self) {
        self.calculated_pan = cgb_pan(self.right, self.left);
    }

    /// Commit the last-recomputed pan to the rendered route, matching the
    /// `CGB_CHANNEL_MO_VOL` NR51 write (`m4a.c:1205-1208`).
    pub(super) fn commit_pan(&mut self) {
        self.right_enabled = matches!(self.calculated_pan, Panning::Right | Panning::Both);
        self.left_enabled = matches!(self.calculated_pan, Panning::Left | Panning::Both);
    }

    pub(super) fn envelope_goal(self) -> u8 {
        cgb_envelope_goal(self.right, self.left, cgb_pan(self.right, self.left))
    }

    pub(super) fn accumulate(self, contribution: i32, output: &mut StereoAcc) {
        if self.right_enabled {
            output.1 += contribution;
        }
        if self.left_enabled {
            output.0 += contribution;
        }
    }
}
