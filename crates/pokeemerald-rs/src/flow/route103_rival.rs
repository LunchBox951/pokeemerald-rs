//! Selects the trainer identity for the scripted Route 103 rival battle.

use assets::trainers::TrainerId;
use assets::SpeciesId;

const TREECKO_STARTER_VALUE: u16 = 0;
const TORCHIC_STARTER_VALUE: u16 = 1;
const MUDKIP_STARTER_VALUE: u16 = 2;

/// The starter recorded for the player in `VAR_STARTER_MON`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerStarter {
    /// Treecko.
    Treecko,
    /// Torchic.
    Torchic,
    /// Mudkip.
    Mudkip,
}

impl PlayerStarter {
    /// Returns the starter represented by `species`, or `None` for any other species.
    #[must_use]
    pub const fn from_species(species: SpeciesId) -> Option<Self> {
        match species {
            SpeciesId::TREECKO => Some(Self::Treecko),
            SpeciesId::TORCHIC => Some(Self::Torchic),
            SpeciesId::MUDKIP => Some(Self::Mudkip),
            _ => None,
        }
    }

    /// Decodes a `VAR_STARTER_MON` value, returning `None` outside the starter range.
    #[must_use]
    pub const fn from_var(value: u16) -> Option<Self> {
        match value {
            TREECKO_STARTER_VALUE => Some(Self::Treecko),
            TORCHIC_STARTER_VALUE => Some(Self::Torchic),
            MUDKIP_STARTER_VALUE => Some(Self::Mudkip),
            _ => None,
        }
    }

    /// Encodes this starter for `VAR_STARTER_MON`.
    #[must_use]
    pub const fn var_value(self) -> u16 {
        match self {
            Self::Treecko => TREECKO_STARTER_VALUE,
            Self::Torchic => TORCHIC_STARTER_VALUE,
            Self::Mudkip => MUDKIP_STARTER_VALUE,
        }
    }
}

/// The opposite-gender protagonist who acts as the player's rival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rival {
    /// Brendan, the rival of a female player.
    Brendan,
    /// May, the rival of a male player.
    May,
}

impl Rival {
    /// Returns the rival selected for the saved player gender.
    ///
    /// [`engine::save::PlayerGender::Other`] has no rival and returns `None`.
    #[must_use]
    pub const fn for_gender(gender: engine::save::PlayerGender) -> Option<Self> {
        match gender {
            engine::save::PlayerGender::Male => Some(Self::May),
            engine::save::PlayerGender::Female => Some(Self::Brendan),
            engine::save::PlayerGender::Other(_) => None,
        }
    }
}

/// Returns the Route 103 trainer identity for the rival and player's starter.
///
/// Each trainer identity's suffix names the player's starter, not the rival's species.
#[must_use]
pub const fn route103_rival_for(rival: Rival, starter: PlayerStarter) -> TrainerId {
    match (rival, starter) {
        (Rival::Brendan, PlayerStarter::Mudkip) => TrainerId::BRENDAN_ROUTE_103_MUDKIP,
        (Rival::Brendan, PlayerStarter::Treecko) => TrainerId::BRENDAN_ROUTE_103_TREECKO,
        (Rival::Brendan, PlayerStarter::Torchic) => TrainerId::BRENDAN_ROUTE_103_TORCHIC,
        (Rival::May, PlayerStarter::Mudkip) => TrainerId::MAY_ROUTE_103_MUDKIP,
        (Rival::May, PlayerStarter::Treecko) => TrainerId::MAY_ROUTE_103_TREECKO,
        (Rival::May, PlayerStarter::Torchic) => TrainerId::MAY_ROUTE_103_TORCHIC,
    }
}

#[cfg(test)]
mod tests;
