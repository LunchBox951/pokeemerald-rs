//! The checked-in manifest of every `MUS_*` and `SE_*` song the early-game checkpoint reaches.
//!
//! Each entry names a `sound/song_table.inc` row and the music player (`Song.ms`) that row
//! assigns. The pack name is the row's lowercase label, so `audio/song/<symbol>` and
//! `sound/songs/midi/<symbol>.mid` follow from it. Extraction checks every entry against the
//! checkout's song table and `midi.cfg` and fails on any disagreement. `audit` records the
//! upstream call sites that justify each entry.
//!
//! The checkpoint is Littleroot, Oldale, Route 101, and the western part of Route 103, with
//! the systems upstream makes available there: the title, intro, and naming screens, the field,
//! menus, shops, Pokemon Center, PC, battles, captures, growth, and the move animations the
//! local species can reach. Cries are not song rows and are excluded, as are the multiplayer
//! and later-story songs, Route 103's eastern trainers, and the Match Call registration.

#[cfg(test)]
mod audit;
#[cfg(test)]
mod tests;

use super::error::MidiError;

const MUSIC_SYMBOL_PREFIX: &str = "mus_";
const EFFECT_SYMBOL_PREFIX: &str = "se_";
const PACK_ID_PREFIX: &str = "audio/song/";

/// Where a manifest song's voicegroup label comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VoiceGroupSource {
    /// The `midi.cfg` entry's `-G` flag, which it must carry.
    Configured,
    /// The `-G` flag when present, else upstream's `dummy` default (`tools/mid2agb/main.cpp:36-43`).
    /// Valid only for a song that compiles to no tracks.
    DefaultWhenSilent,
}

/// One song the checkpoint reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SongSource {
    /// The lowercase `song_table.inc` label, also the pack name.
    pub(super) symbol: &'static str,
    /// The `Song.ms` assignment `song_table.inc` must give this row.
    pub(super) player: u16,
    pub(super) voicegroup: VoiceGroupSource,
}

impl SongSource {
    const fn new(symbol: &'static str, player: u16) -> Self {
        Self {
            symbol,
            player,
            voicegroup: VoiceGroupSource::Configured,
        }
    }

    /// The silent `mus_dummy` row, whose `midi.cfg` entry has no `-G` flag.
    const fn silent_default_group(symbol: &'static str, player: u16) -> Self {
        Self {
            symbol,
            player,
            voicegroup: VoiceGroupSource::DefaultWhenSilent,
        }
    }

    pub(super) fn pack_id(&self) -> String {
        format!("{PACK_ID_PREFIX}{}", self.symbol)
    }

    pub(super) fn midi_filename(&self) -> String {
        format!("{}.mid", self.symbol)
    }
}

/// Rejects a malformed symbol, or one listed twice.
///
/// # Errors
///
/// [`MidiError::ManifestSymbolMalformed`] or [`MidiError::ManifestDuplicateSong`].
pub(super) fn check_manifest(sources: &[SongSource]) -> Result<(), MidiError> {
    let mut seen = std::collections::BTreeSet::new();
    for source in sources {
        let symbol = source.symbol;
        let stem = symbol
            .strip_prefix(MUSIC_SYMBOL_PREFIX)
            .or_else(|| symbol.strip_prefix(EFFECT_SYMBOL_PREFIX));
        let is_label = stem.is_some_and(|stem| {
            !stem.is_empty()
                && stem
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        if !is_label {
            return Err(MidiError::ManifestSymbolMalformed(symbol.to_owned()));
        }
        if !seen.insert(symbol) {
            return Err(MidiError::ManifestDuplicateSong(symbol.to_owned()));
        }
    }
    Ok(())
}

/// Every checkpoint song: music and silence, then menu, field, and battle sounds, then move,
/// status, and evolution animation sounds.
pub(super) const CHECKPOINT_SONGS: [SongSource; 150] = [
    // Map music, title and intro, fanfares, battle themes, and the rival and TV encounters.
    SongSource::new("mus_birch_lab", 0),
    SongSource::new("mus_caught", 0),
    SongSource::silent_default_group("mus_dummy", 0),
    SongSource::new("mus_encounter_brendan", 0),
    SongSource::new("mus_encounter_interviewer", 0),
    SongSource::new("mus_encounter_may", 0),
    SongSource::new("mus_evolution", 0),
    SongSource::new("mus_evolution_intro", 2),
    SongSource::new("mus_evolved", 2),
    SongSource::new("mus_follow_me", 0),
    SongSource::new("mus_heal", 2),
    SongSource::new("mus_help", 0),
    SongSource::new("mus_intro", 0),
    SongSource::new("mus_intro_battle", 0),
    SongSource::new("mus_level_up", 2),
    SongSource::new("mus_littleroot", 0),
    SongSource::new("mus_obtain_item", 2),
    SongSource::new("mus_oldale", 0),
    SongSource::new("mus_poke_center", 0),
    SongSource::new("mus_poke_mart", 0),
    SongSource::new("mus_rg_caught_intro", 2),
    SongSource::new("mus_route101", 0),
    SongSource::new("mus_route122", 0),
    SongSource::new("mus_title", 0),
    SongSource::new("mus_victory_trainer", 0),
    SongSource::new("mus_victory_wild", 0),
    SongSource::new("mus_vs_rival", 0),
    SongSource::new("mus_vs_wild", 0),
    // Menu, field, battle, and ball sounds that are not move animations.
    SongSource::new("se_ball", 1),
    SongSource::new("se_ball_bounce_1", 1),
    SongSource::new("se_ball_bounce_2", 1),
    SongSource::new("se_ball_bounce_3", 1),
    SongSource::new("se_ball_bounce_4", 1),
    SongSource::new("se_ball_open", 1),
    SongSource::new("se_ball_throw", 1),
    SongSource::new("se_ball_trade", 2),
    SongSource::new("se_ball_tray_ball", 1),
    SongSource::new("se_ball_tray_enter", 1),
    SongSource::new("se_ball_tray_exit", 2),
    SongSource::new("se_boo", 1),
    SongSource::new("se_click", 1),
    SongSource::new("se_dex_page", 1),
    SongSource::new("se_dex_scroll", 1),
    SongSource::new("se_dex_search", 1),
    SongSource::new("se_door", 1),
    SongSource::new("se_effective", 1),
    SongSource::new("se_escalator", 1),
    SongSource::new("se_exit", 1),
    SongSource::new("se_exp", 1),
    SongSource::new("se_exp_max", 1),
    SongSource::new("se_failure", 1),
    SongSource::new("se_faint", 1),
    SongSource::new("se_flee", 1),
    SongSource::new("se_intro_blast", 1),
    SongSource::new("se_ledge", 1),
    SongSource::new("se_low_health", 3),
    SongSource::new("se_not_effective", 1),
    SongSource::new("se_pc_login", 1),
    SongSource::new("se_pc_off", 1),
    SongSource::new("se_pc_on", 1),
    SongSource::new("se_pin", 1),
    SongSource::new("se_repel", 1),
    SongSource::new("se_rg_ball_click", 1),
    SongSource::new("se_rg_card_flip", 1),
    SongSource::new("se_rg_card_flipping", 1),
    SongSource::new("se_rg_card_open", 1),
    SongSource::new("se_save", 1),
    SongSource::new("se_select", 2),
    SongSource::new("se_shiny", 1),
    SongSource::new("se_shop", 1),
    SongSource::new("se_sliding_door", 1),
    SongSource::new("se_success", 1),
    SongSource::new("se_super_effective", 1),
    SongSource::new("se_truck_door", 1),
    SongSource::new("se_truck_move", 1),
    SongSource::new("se_truck_stop", 1),
    SongSource::new("se_truck_unload", 2),
    SongSource::new("se_use_item", 1),
    SongSource::new("se_wall_hit", 2),
    SongSource::new("se_win_open", 1),
    // Move, status, and evolution animation sounds.
    SongSource::new("se_m_absorb", 1),
    SongSource::new("se_m_absorb_2", 1),
    SongSource::new("se_m_attract", 1),
    SongSource::new("se_m_attract2", 1),
    SongSource::new("se_m_belly_drum", 1),
    SongSource::new("se_m_bite", 1),
    SongSource::new("se_m_bubble", 1),
    SongSource::new("se_m_bubble2", 1),
    SongSource::new("se_m_bubble3", 1),
    SongSource::new("se_m_bubble_beam2", 1),
    SongSource::new("se_m_charm", 1),
    SongSource::new("se_m_comet_punch", 1),
    SongSource::new("se_m_crabhammer", 1),
    SongSource::new("se_m_cut", 1),
    SongSource::new("se_m_detect", 2),
    SongSource::new("se_m_dig", 1),
    SongSource::new("se_m_dizzy_punch", 1),
    SongSource::new("se_m_double_slap", 1),
    SongSource::new("se_m_double_team", 1),
    SongSource::new("se_m_dragon_rage", 1),
    SongSource::new("se_m_earthquake", 2),
    SongSource::new("se_m_ember", 1),
    SongSource::new("se_m_fire_punch", 1),
    SongSource::new("se_m_flamethrower", 1),
    SongSource::new("se_m_flame_wheel", 1),
    SongSource::new("se_m_giga_drain", 1),
    SongSource::new("se_m_gust", 2),
    SongSource::new("se_m_gust2", 2),
    SongSource::new("se_m_harden", 1),
    SongSource::new("se_m_headbutt", 1),
    SongSource::new("se_m_horn_attack", 1),
    SongSource::new("se_m_hydro_pump", 1),
    SongSource::new("se_m_jump_kick", 1),
    SongSource::new("se_m_leer", 1),
    SongSource::new("se_m_mega_kick", 1),
    SongSource::new("se_m_mega_kick2", 1),
    SongSource::new("se_m_metronome", 1),
    SongSource::new("se_m_mist", 1),
    SongSource::new("se_m_moonlight", 1),
    SongSource::new("se_m_morning_sun", 1),
    SongSource::new("se_m_petal_dance", 1),
    SongSource::new("se_m_poison_powder", 1),
    SongSource::new("se_m_psybeam", 1),
    SongSource::new("se_m_psybeam2", 1),
    SongSource::new("se_m_razor_wind", 1),
    SongSource::new("se_m_razor_wind2", 1),
    SongSource::new("se_m_reflect", 1),
    SongSource::new("se_m_sacred_fire2", 2),
    SongSource::new("se_m_sand_attack", 1),
    SongSource::new("se_m_scratch", 1),
    SongSource::new("se_m_screech", 1),
    SongSource::new("se_m_sketch", 1),
    SongSource::new("se_m_sky_uppercut", 2),
    SongSource::new("se_m_snore", 1),
    SongSource::new("se_m_spit_up", 1),
    SongSource::new("se_m_stat_decrease", 1),
    SongSource::new("se_m_stat_increase", 1),
    SongSource::new("se_m_string_shot", 1),
    SongSource::new("se_m_string_shot2", 1),
    SongSource::new("se_m_supersonic", 1),
    SongSource::new("se_m_swagger", 1),
    SongSource::new("se_m_swagger2", 1),
    SongSource::new("se_m_tail_whip", 2),
    SongSource::new("se_m_take_down", 2),
    SongSource::new("se_m_thunderbolt2", 1),
    SongSource::new("se_m_toxic", 1),
    SongSource::new("se_m_vital_throw", 1),
    SongSource::new("se_m_vital_throw2", 1),
    SongSource::new("se_m_whirlpool", 1),
    SongSource::new("se_m_wing_attack", 1),
];
