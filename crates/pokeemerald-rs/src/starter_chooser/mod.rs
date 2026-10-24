//! The starter chooser (`pokeemerald/src/starter_choose.c`): Birch's bag with
//! three Poké Balls, a hand cursor, a Pokémon preview and a YES/NO
//! confirmation.
//!
//! [`StarterChooserScene`] is the whole scene as a value. A caller builds it
//! from the pack, calls [`step`](StarterChooserScene::step) once per frame with
//! the frame's [`ButtonState`], presents [`compose`](StarterChooserScene::compose),
//! and plays the [`StarterSoundRequest`]s each step returns. The step that
//! confirms returns the typed [`StarterChoice`], upstream's write to
//! `gSpecialVar_Result` before `SetMainCallback2(gMain.savedCallback)`
//! (`starter_choose.c:546-550`); B and NO only cancel the preview.
//!
//! Frame order follows `CB2_StarterChoose` (`:465-472`): one task handler, the
//! sprite callbacks and animations, then the palette update. The scene opens
//! with the all-palette black fade-in `CB2_ChooseStarter` begins (`:420`),
//! and, as upstream, does not gate input on it.
//!
//! Not modelled: binding the chooser into the field script (`ChooseStarter`),
//! creating the chosen Pokémon, and playing the requested sounds.

use assets::pack::PackError;
use assets::{AssetError, AssetPack, SpeciesId, StarterSpecies};
use platform::ButtonState;
use rendering::{Framebuffer, NormalPaletteFade, PaletteFadeTarget, RenderError};

mod graphics;
mod sprites;
mod state;
mod text;

#[cfg(test)]
mod tests;

pub use state::{StarterChooserPhase, StarterSoundRequest};

use graphics::{Graphics, SceneFrame};
use state::Chooser;
use text::TextAssets;

/// The default window frame (`optionsWindowFrameType` zero).
const DEFAULT_WINDOW_FRAME: u8 = 0;

/// The three starters, in the chooser's left-to-right order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StarterChoice {
    /// The left ball.
    Treecko,
    /// The middle ball, selected when the chooser opens.
    Torchic,
    /// The right ball.
    Mudkip,
}

impl StarterChoice {
    /// Every starter in ball order (`starter_choose.c:99-118`).
    pub const ALL: [Self; 3] = [Self::Treecko, Self::Torchic, Self::Mudkip];

    /// The ball's position from the left, upstream's `tStarterSelection`.
    #[must_use]
    pub const fn ordinal(self) -> usize {
        match self {
            Self::Treecko => 0,
            Self::Torchic => 1,
            Self::Mudkip => 2,
        }
    }

    /// The species this choice gives.
    #[must_use]
    pub const fn species_id(self) -> SpeciesId {
        self.starter_species().species_id()
    }

    /// The asset handle's name for this starter.
    #[must_use]
    pub const fn starter_species(self) -> StarterSpecies {
        match self {
            Self::Treecko => StarterSpecies::Treecko,
            Self::Torchic => StarterSpecies::Torchic,
            Self::Mudkip => StarterSpecies::Mudkip,
        }
    }
}

/// Why the chooser could not be built.
#[derive(Debug)]
pub enum StarterChooserError {
    /// A chooser, text or font entry is missing or malformed.
    Pack(PackError),
    /// A font sheet or species name could not be read.
    Asset(AssetError),
    /// Decoded graphics did not fit the rendering types.
    Render(RenderError),
}

impl std::fmt::Display for StarterChooserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "starter chooser: {err}"),
            Self::Asset(err) => write!(f, "starter chooser: {err}"),
            Self::Render(err) => write!(f, "starter chooser: {err}"),
        }
    }
}

impl std::error::Error for StarterChooserError {}

impl From<PackError> for StarterChooserError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<AssetError> for StarterChooserError {
    fn from(err: AssetError) -> Self {
        Self::Asset(err)
    }
}

impl From<RenderError> for StarterChooserError {
    fn from(err: RenderError) -> Self {
        Self::Render(err)
    }
}

/// What one [`step`](StarterChooserScene::step) produced.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StarterChooserStep {
    /// The confirmed starter, on the frame the player answers YES.
    pub choice: Option<StarterChoice>,
    /// Sounds to request this frame, in order.
    pub sounds: Vec<StarterSoundRequest>,
}

/// The chooser scene.
#[derive(Debug)]
pub struct StarterChooserScene {
    graphics: Graphics,
    text: TextAssets,
    chooser: Chooser,
    fade: NormalPaletteFade,
    fade_updates: u32,
    unfaded_mon_palette: bool,
}

impl StarterChooserScene {
    /// Builds the chooser with the default window frame.
    ///
    /// # Errors
    ///
    /// [`StarterChooserError`] when a graphic, font or frame is
    /// missing or malformed.
    pub fn from_pack(pack: &AssetPack) -> Result<Self, StarterChooserError> {
        Self::from_pack_with_window_frame(pack, DEFAULT_WINDOW_FRAME)
    }

    /// Builds the chooser bordered with the player's saved window frame.
    ///
    /// # Errors
    ///
    /// As [`from_pack`](Self::from_pack).
    pub fn from_pack_with_window_frame(
        pack: &AssetPack,
        window_frame: u8,
    ) -> Result<Self, StarterChooserError> {
        Ok(Self {
            graphics: Graphics::from_pack(pack)?,
            text: TextAssets::from_pack(pack, window_frame)?,
            chooser: Chooser::new(),
            // `BeginNormalPaletteFade` has already updated once.
            fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
            fade_updates: 1,
            unfaded_mon_palette: false,
        })
    }

    /// Runs one frame and returns its sounds and, when the player confirmed,
    /// the choice. Once the choice is returned the scene is finished and later
    /// steps do nothing.
    pub fn step(&mut self, buttons: ButtonState) -> StarterChooserStep {
        if self.chooser.phase == StarterChooserPhase::Finished {
            return StarterChooserStep::default();
        }
        let frame = self.chooser.step(buttons);
        if frame.preview_created {
            self.unfaded_mon_palette = true;
        }
        self.update_fade();
        StarterChooserStep {
            choice: frame.choice,
            sounds: frame.sounds,
        }
    }

    /// `UpdatePaletteFade`. A palette loaded since the last object update
    /// stays unfaded until the update that blends the object half, which
    /// alternates with the background half after the update `begin` ran.
    fn update_fade(&mut self) {
        const LAST_BLEND_UPDATE: u32 = 17;
        self.fade.update();
        let updates = self.fade_updates;
        self.fade_updates += 1;
        let blended_objects = updates % 2 == 1 || updates > LAST_BLEND_UPDATE;
        if blended_objects {
            self.unfaded_mon_palette = false;
        }
    }

    /// The starter the hand is on.
    #[must_use]
    pub const fn selected(&self) -> StarterChoice {
        self.chooser.selected
    }

    /// The task handler the next [`step`](Self::step) runs.
    #[must_use]
    pub const fn phase(&self) -> StarterChooserPhase {
        self.chooser.phase
    }

    /// Composes the current frame without advancing the scene.
    #[must_use]
    pub fn compose(&self) -> Framebuffer {
        let sprites = &self.chooser.sprites;
        let oam = sprites.oam_entries(self.chooser.selected);
        let matrices = sprites::matrices(sprites.preview);
        let frame = SceneFrame {
            oam: &oam,
            matrices: &matrices,
            preview_species: sprites
                .preview
                .map_or(self.chooser.selected, sprites::Preview::choice),
            label_window: self.chooser.label.map(text::label_window_rect),
            bg_fade: self.fade.bg_blend(),
            obj_fade: self.fade.obj_blend(),
            unfaded_mon_palette: self.unfaded_mon_palette,
        };
        let mut fb = self.graphics.compose(&frame);
        let colors = self.text.colors(|color| {
            rendering::PaletteColorTransform::transform(&self.fade.bg_blend(), color)
        });
        self.text.draw(
            &mut fb,
            &colors,
            self.chooser.label,
            self.chooser.bottom,
            self.chooser.menu,
        );
        fb
    }

    /// Composes the current frame in the platform's presentation format.
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}
