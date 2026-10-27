//! The bag picture's affine shake on list cursor movement (`ShakeBagSprite`,
//! `item_menu_icons.c:477-494`): the sprite's affine animation, run as
//! `sprite.c` runs it (`AnimateSprites`, `sprite.c:308-323`), since which
//! moves restart the shake depends on when the animation counts as ended.

/// One command of a sprite affine animation. Only the rotation is
/// animated: the shake's scale deltas are 0 and the scale stays 256.
#[derive(Debug, Clone, Copy)]
enum Command {
    /// `AFFINEANIMCMD_FRAME(_, _, rotation, duration)`: turns by `rotation`
    /// 1/256 turns every tick of `duration`, or sets it when `duration` is 0.
    Frame {
        rotation: i16,
        duration: u8,
    },
    End,
}

/// `sSpriteAffineAnim_BagNormal` (`item_menu_icons.c:104-108`).
const NORMAL: [Command; 2] = [
    Command::Frame {
        rotation: 0,
        duration: 0,
    },
    Command::End,
];

/// `sSpriteAffineAnim_BagShake` (`item_menu_icons.c:110-117`); the frames'
/// 254 is -2 as a signed byte.
const SHAKE: [Command; 5] = [
    Command::Frame {
        rotation: -2,
        duration: 2,
    },
    Command::Frame {
        rotation: 2,
        duration: 4,
    },
    Command::Frame {
        rotation: -2,
        duration: 4,
    },
    Command::Frame {
        rotation: 2,
        duration: 2,
    },
    Command::End,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Animation {
    Normal,
    Shake,
}

impl Animation {
    fn commands(self) -> &'static [Command] {
        match self {
            Self::Normal => &NORMAL,
            Self::Shake => &SHAKE,
        }
    }
}

/// The bag sprite's affine state and the callback that resets it.
#[derive(Debug, Clone, Copy)]
pub(super) struct BagShake {
    animation: Animation,
    index: usize,
    delay: u8,
    /// Counted in 1/256 turns, positive counter-clockwise as the rotating
    /// ball's is.
    rotation: i16,
    /// `affineAnimBeginning`: the animation was just started.
    beginning: bool,
    /// `affineAnimEnded`: the only state that lets a move start a shake.
    ended: bool,
    /// Whether the sprite callback is `SpriteCB_ShakeBagSprite`, which
    /// starts the normal animation once a shake ends. (A pocket switch
    /// replaces it, `item_menu_icons.c:446-458`, but the switch drops every
    /// move for longer than the shake's cooldown, so nothing can tell.)
    resets: bool,
}

impl Default for BagShake {
    /// A bag that has been on screen long enough to have settled: the normal
    /// animation run to its end, so the first move shakes it.
    fn default() -> Self {
        Self {
            animation: Animation::Normal,
            index: 0,
            delay: 0,
            rotation: 0,
            beginning: false,
            ended: true,
            resets: false,
        }
    }
}

impl BagShake {
    fn begin(&mut self, animation: Animation) {
        *self = Self {
            animation,
            index: 0,
            delay: 0,
            rotation: 0,
            beginning: true,
            ended: false,
            ..*self
        };
    }

    /// `ShakeBagSprite`: the list's cursor moved. A shake already under way,
    /// or the normal animation still starting, is not interrupted.
    pub(super) fn start(&mut self) {
        if self.ended {
            self.begin(Animation::Shake);
            self.resets = true;
        }
    }

    /// One frame of the sprite update: its callback, then its affine
    /// animation (`sprite.c:316-320`).
    pub(super) fn update(&mut self) {
        if self.resets && self.ended {
            self.begin(Animation::Normal);
            self.resets = false;
        }
        if self.beginning {
            self.beginning = false;
            self.index = 0;
            self.delay = 0;
            self.apply_frame();
        } else if self.delay > 0 {
            self.delay -= 1;
            self.rotate_by_frame();
        } else {
            self.index += 1;
            if let Some(Command::Frame { .. }) = self.animation.commands().get(self.index) {
                self.apply_frame();
            } else {
                // The end command stays put, as `AffineAnimCmd_end` steps the
                // index back to it.
                self.ended = true;
                self.index = self.index.saturating_sub(1);
            }
        }
    }

    /// `ApplyAffineAnimFrame`: a timed frame turns the sprite now and again
    /// each tick that follows; a frame of duration 0 sets the angle.
    fn apply_frame(&mut self) {
        if let Some(&Command::Frame { rotation, duration }) =
            self.animation.commands().get(self.index)
        {
            if duration > 0 {
                self.rotation += rotation;
                self.delay = duration - 1;
            } else {
                self.rotation = rotation;
                self.delay = 0;
            }
        }
    }

    fn rotate_by_frame(&mut self) {
        if let Some(&Command::Frame { rotation, .. }) = self.animation.commands().get(self.index) {
            self.rotation += rotation;
        }
    }

    /// The bag picture's turn as a 256-unit angle.
    pub(super) fn angle(self) -> u8 {
        u8::try_from(self.rotation.rem_euclid(256)).unwrap_or(0)
    }
}
