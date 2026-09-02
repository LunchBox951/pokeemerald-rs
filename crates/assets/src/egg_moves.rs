//! Typed egg-move lists and lookup by species.

use crate::battle_moves::MoveId;
use crate::error::AssetError;
use crate::species::SpeciesId;

/// Offset that distinguishes a species sentinel from a move in the encoded stream.
pub const EGG_MOVES_SPECIES_OFFSET: u16 = 20000;

/// Marker that ends the encoded egg-move stream.
pub const EGG_MOVES_TERMINATOR: u16 = 0xFFFF;

/// One species' ordered egg moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EggMoveList {
    /// The species that learns these moves.
    pub species: SpeciesId,
    /// The egg moves in canonical order.
    pub moves: &'static [MoveId],
}

impl EggMoveList {
    /// Returns the species that learns these moves.
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Returns the egg moves in canonical order.
    #[must_use]
    pub const fn moves(&self) -> &'static [MoveId] {
        self.moves
    }

    /// Returns the number of egg moves.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.moves.len()
    }

    /// Returns whether the list contains no moves.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Returns whether the species can learn `mv` as an egg move.
    #[must_use]
    pub fn teaches(&self, mv: MoveId) -> bool {
        self.moves.contains(&mv)
    }
}

macro_rules! define_egg_moves {
    ($($species:ident => [$($egg_move:ident),+ $(,)?]),+ $(,)?) => {
        const EGG_MOVES: &[EggMoveList] = &[
            $(EggMoveList {
                species: SpeciesId::$species,
                moves: &[$(MoveId::$egg_move),+],
            },)+
        ];
    };
}

#[rustfmt::skip]
define_egg_moves! {
    BULBASAUR => [LIGHT_SCREEN, SKULL_BASH, SAFEGUARD, CHARM, PETAL_DANCE, MAGICAL_LEAF, GRASS_WHISTLE, CURSE],
    CHARMANDER => [BELLY_DRUM, ANCIENT_POWER, ROCK_SLIDE, BITE, OUTRAGE, BEAT_UP, SWORDS_DANCE, DRAGON_DANCE],
    SQUIRTLE => [MIRROR_COAT, HAZE, MIST, FORESIGHT, FLAIL, REFRESH, MUD_SPORT, YAWN],
    PIDGEY => [PURSUIT, FAINT_ATTACK, FORESIGHT, STEEL_WING, AIR_CUTTER],
    RATTATA => [SCREECH, FLAME_WHEEL, FURY_SWIPES, BITE, COUNTER, REVERSAL, UPROAR, SWAGGER],
    SPEAROW => [FAINT_ATTACK, FALSE_SWIPE, SCARY_FACE, QUICK_ATTACK, TRI_ATTACK, ASTONISH, SKY_ATTACK],
    EKANS => [PURSUIT, SLAM, SPITE, BEAT_UP, POISON_FANG],
    SANDSHREW => [FLAIL, SAFEGUARD, COUNTER, RAPID_SPIN, ROCK_SLIDE, METAL_CLAW, SWORDS_DANCE, CRUSH_CLAW],
    NIDORAN_F => [SUPERSONIC, DISABLE, TAKE_DOWN, FOCUS_ENERGY, CHARM, COUNTER, BEAT_UP],
    NIDORAN_M => [COUNTER, DISABLE, SUPERSONIC, TAKE_DOWN, AMNESIA, CONFUSION, BEAT_UP],
    VULPIX => [FAINT_ATTACK, HYPNOSIS, FLAIL, SPITE, DISABLE, HOWL, PSYCH_UP, HEAT_WAVE],
    ZUBAT => [QUICK_ATTACK, PURSUIT, FAINT_ATTACK, GUST, WHIRLWIND, CURSE],
    ODDISH => [SWORDS_DANCE, RAZOR_LEAF, FLAIL, SYNTHESIS, CHARM, INGRAIN],
    PARAS => [FALSE_SWIPE, SCREECH, COUNTER, PSYBEAM, FLAIL, SWEET_SCENT, LIGHT_SCREEN, PURSUIT],
    VENONAT => [BATON_PASS, SCREECH, GIGA_DRAIN, SIGNAL_BEAM],
    DIGLETT => [FAINT_ATTACK, SCREECH, ANCIENT_POWER, PURSUIT, BEAT_UP, UPROAR, ROCK_SLIDE],
    MEOWTH => [SPITE, CHARM, HYPNOSIS, AMNESIA, PSYCH_UP, ASSIST],
    PSYDUCK => [HYPNOSIS, PSYBEAM, FORESIGHT, LIGHT_SCREEN, FUTURE_SIGHT, PSYCHIC, CROSS_CHOP, REFRESH],
    MANKEY => [ROCK_SLIDE, FORESIGHT, MEDITATE, COUNTER, REVERSAL, BEAT_UP, REVENGE, SMELLING_SALT],
    GROWLITHE => [BODY_SLAM, SAFEGUARD, CRUNCH, THRASH, FIRE_SPIN, HOWL, HEAT_WAVE],
    POLIWAG => [MIST, SPLASH, BUBBLE_BEAM, HAZE, MIND_READER, WATER_SPORT, ICE_BALL],
    ABRA => [ENCORE, BARRIER, KNOCK_OFF, FIRE_PUNCH, THUNDER_PUNCH, ICE_PUNCH],
    MACHOP => [LIGHT_SCREEN, MEDITATE, ROLLING_KICK, ENCORE, SMELLING_SALT, COUNTER, ROCK_SLIDE],
    BELLSPROUT => [SWORDS_DANCE, ENCORE, REFLECT, SYNTHESIS, LEECH_LIFE, INGRAIN, MAGICAL_LEAF],
    TENTACOOL => [AURORA_BEAM, MIRROR_COAT, RAPID_SPIN, HAZE, SAFEGUARD, CONFUSE_RAY],
    GEODUDE => [MEGA_PUNCH, ROCK_SLIDE, BLOCK],
    PONYTA => [FLAME_WHEEL, THRASH, DOUBLE_KICK, HYPNOSIS, CHARM, DOUBLE_EDGE],
    SLOWPOKE => [SAFEGUARD, BELLY_DRUM, FUTURE_SIGHT, STOMP, MUD_SPORT, SLEEP_TALK, SNORE],
    FARFETCHD => [STEEL_WING, FORESIGHT, MIRROR_MOVE, GUST, QUICK_ATTACK, FLAIL, FEATHER_DANCE, CURSE],
    DODUO => [QUICK_ATTACK, SUPERSONIC, HAZE, FAINT_ATTACK, FLAIL, ENDEAVOR],
    SEEL => [LICK, PERISH_SONG, DISABLE, HORN_DRILL, SLAM, ENCORE, FAKE_OUT, ICICLE_SPEAR],
    GRIMER => [HAZE, MEAN_LOOK, LICK, IMPRISON, CURSE, SHADOW_PUNCH, EXPLOSION],
    SHELLDER => [BUBBLE_BEAM, TAKE_DOWN, BARRIER, RAPID_SPIN, SCREECH, ICICLE_SPEAR],
    GASTLY => [PSYWAVE, PERISH_SONG, HAZE, ASTONISH, WILL_O_WISP, GRUDGE, EXPLOSION],
    ONIX => [ROCK_SLIDE, FLAIL, EXPLOSION, BLOCK],
    DROWZEE => [BARRIER, ASSIST, ROLE_PLAY, FIRE_PUNCH, THUNDER_PUNCH, ICE_PUNCH],
    KRABBY => [DIG, HAZE, AMNESIA, FLAIL, SLAM, KNOCK_OFF, SWORDS_DANCE],
    EXEGGCUTE => [SYNTHESIS, MOONLIGHT, REFLECT, ANCIENT_POWER, PSYCH_UP, INGRAIN, CURSE],
    CUBONE => [ROCK_SLIDE, ANCIENT_POWER, BELLY_DRUM, SCREECH, SKULL_BASH, PERISH_SONG, SWORDS_DANCE],
    LICKITUNG => [BELLY_DRUM, MAGNITUDE, BODY_SLAM, CURSE, SMELLING_SALT, SLEEP_TALK, SNORE, SUBSTITUTE],
    KOFFING => [SCREECH, PSYWAVE, PSYBEAM, DESTINY_BOND, PAIN_SPLIT, WILL_O_WISP],
    RHYHORN => [CRUNCH, REVERSAL, ROCK_SLIDE, COUNTER, MAGNITUDE, SWORDS_DANCE, CURSE, CRUSH_CLAW],
    CHANSEY => [PRESENT, METRONOME, HEAL_BELL, AROMATHERAPY, SUBSTITUTE],
    TANGELA => [FLAIL, CONFUSION, MEGA_DRAIN, REFLECT, AMNESIA, LEECH_SEED, NATURE_POWER],
    KANGASKHAN => [STOMP, FORESIGHT, FOCUS_ENERGY, SAFEGUARD, DISABLE, COUNTER, CRUSH_CLAW, SUBSTITUTE],
    HORSEA => [FLAIL, AURORA_BEAM, OCTAZOOKA, DISABLE, SPLASH, DRAGON_RAGE, DRAGON_BREATH],
    GOLDEEN => [PSYBEAM, HAZE, HYDRO_PUMP, SLEEP_TALK, MUD_SPORT],
    MR_MIME => [FUTURE_SIGHT, HYPNOSIS, MIMIC, PSYCH_UP, FAKE_OUT, TRICK],
    SCYTHER => [COUNTER, SAFEGUARD, BATON_PASS, RAZOR_WIND, REVERSAL, LIGHT_SCREEN, ENDURE, SILVER_WIND],
    PINSIR => [FURY_ATTACK, FLAIL, FALSE_SWIPE, FAINT_ATTACK],
    LAPRAS => [FORESIGHT, SUBSTITUTE, TICKLE, REFRESH, DRAGON_DANCE, CURSE, SLEEP_TALK, HORN_DRILL],
    EEVEE => [CHARM, FLAIL, ENDURE, CURSE, TICKLE, WISH],
    OMANYTE => [BUBBLE_BEAM, AURORA_BEAM, SLAM, SUPERSONIC, HAZE, ROCK_SLIDE, SPIKES],
    KABUTO => [BUBBLE_BEAM, AURORA_BEAM, RAPID_SPIN, DIG, FLAIL, KNOCK_OFF, CONFUSE_RAY],
    AERODACTYL => [WHIRLWIND, PURSUIT, FORESIGHT, STEEL_WING, DRAGON_BREATH, CURSE],
    SNORLAX => [LICK, CHARM, DOUBLE_EDGE, CURSE, FISSURE, SUBSTITUTE],
    DRATINI => [LIGHT_SCREEN, MIST, HAZE, SUPERSONIC, DRAGON_BREATH, DRAGON_DANCE],
    CHIKORITA => [VINE_WHIP, LEECH_SEED, COUNTER, ANCIENT_POWER, FLAIL, NATURE_POWER, INGRAIN, GRASS_WHISTLE],
    CYNDAQUIL => [FURY_SWIPES, QUICK_ATTACK, REVERSAL, THRASH, FORESIGHT, COVET, HOWL, CRUSH_CLAW],
    TOTODILE => [CRUNCH, THRASH, HYDRO_PUMP, ANCIENT_POWER, ROCK_SLIDE, MUD_SPORT, WATER_SPORT, DRAGON_CLAW],
    SENTRET => [DOUBLE_EDGE, PURSUIT, SLASH, FOCUS_ENERGY, REVERSAL, SUBSTITUTE, TRICK, ASSIST],
    HOOTHOOT => [MIRROR_MOVE, SUPERSONIC, FAINT_ATTACK, WING_ATTACK, WHIRLWIND, SKY_ATTACK, FEATHER_DANCE],
    LEDYBA => [PSYBEAM, BIDE, SILVER_WIND],
    SPINARAK => [PSYBEAM, DISABLE, SONIC_BOOM, BATON_PASS, PURSUIT, SIGNAL_BEAM],
    CHINCHOU => [FLAIL, SCREECH, AMNESIA],
    PICHU => [REVERSAL, BIDE, PRESENT, ENCORE, DOUBLE_SLAP, WISH, CHARGE],
    CLEFFA => [PRESENT, METRONOME, AMNESIA, BELLY_DRUM, SPLASH, MIMIC, WISH, SUBSTITUTE],
    IGGLYBUFF => [PERISH_SONG, PRESENT, FAINT_ATTACK, WISH, FAKE_TEARS],
    TOGEPI => [PRESENT, MIRROR_MOVE, PECK, FORESIGHT, FUTURE_SIGHT, SUBSTITUTE, PSYCH_UP],
    NATU => [HAZE, DRILL_PECK, QUICK_ATTACK, FAINT_ATTACK, STEEL_WING, PSYCH_UP, FEATHER_DANCE, REFRESH],
    MAREEP => [TAKE_DOWN, BODY_SLAM, SAFEGUARD, SCREECH, REFLECT, ODOR_SLEUTH, CHARGE],
    MARILL => [LIGHT_SCREEN, PRESENT, AMNESIA, FUTURE_SIGHT, BELLY_DRUM, PERISH_SONG, SUPERSONIC, SUBSTITUTE],
    SUDOWOODO => [SELF_DESTRUCT],
    HOPPIP => [CONFUSION, ENCORE, DOUBLE_EDGE, REFLECT, AMNESIA, HELPING_HAND, PSYCH_UP],
    AIPOM => [COUNTER, SCREECH, PURSUIT, AGILITY, SPITE, SLAM, DOUBLE_SLAP, BEAT_UP],
    SUNKERN => [GRASS_WHISTLE, ENCORE, LEECH_SEED, NATURE_POWER, CURSE, HELPING_HAND],
    YANMA => [WHIRLWIND, REVERSAL, LEECH_LIFE, SIGNAL_BEAM, SILVER_WIND],
    WOOPER => [BODY_SLAM, ANCIENT_POWER, SAFEGUARD, CURSE, MUD_SPORT, STOCKPILE, SWALLOW, SPIT_UP],
    MURKROW => [WHIRLWIND, DRILL_PECK, MIRROR_MOVE, WING_ATTACK, SKY_ATTACK, CONFUSE_RAY, FEATHER_DANCE, PERISH_SONG],
    MISDREAVUS => [SCREECH, DESTINY_BOND, PSYCH_UP, IMPRISON],
    GIRAFARIG => [TAKE_DOWN, AMNESIA, FORESIGHT, FUTURE_SIGHT, BEAT_UP, PSYCH_UP, WISH, MAGIC_COAT],
    PINECO => [REFLECT, PIN_MISSILE, FLAIL, SWIFT, COUNTER, SAND_TOMB],
    DUNSPARCE => [BIDE, ANCIENT_POWER, ROCK_SLIDE, BITE, HEADBUTT, ASTONISH, CURSE],
    GLIGAR => [METAL_CLAW, WING_ATTACK, RAZOR_WIND, COUNTER, SAND_TOMB],
    SNUBBULL => [METRONOME, FAINT_ATTACK, REFLECT, PRESENT, CRUNCH, HEAL_BELL, SNORE, SMELLING_SALT],
    QWILFISH => [FLAIL, HAZE, BUBBLE_BEAM, SUPERSONIC, ASTONISH],
    SHUCKLE => [SWEET_SCENT],
    HERACROSS => [HARDEN, BIDE, FLAIL, FALSE_SWIPE],
    SNEASEL => [COUNTER, SPITE, FORESIGHT, REFLECT, BITE, CRUSH_CLAW, FAKE_OUT],
    TEDDIURSA => [CRUNCH, TAKE_DOWN, SEISMIC_TOSS, COUNTER, METAL_CLAW, FAKE_TEARS, YAWN, SLEEP_TALK],
    SLUGMA => [ACID_ARMOR, HEAT_WAVE],
    SWINUB => [TAKE_DOWN, BITE, BODY_SLAM, ROCK_SLIDE, ANCIENT_POWER, MUD_SHOT, ICICLE_SPEAR, DOUBLE_EDGE],
    CORSOLA => [ROCK_SLIDE, SCREECH, MIST, AMNESIA, BARRIER, INGRAIN, CONFUSE_RAY, ICICLE_SPEAR],
    REMORAID => [AURORA_BEAM, OCTAZOOKA, SUPERSONIC, HAZE, SCREECH, THUNDER_WAVE, ROCK_BLAST],
    DELIBIRD => [AURORA_BEAM, QUICK_ATTACK, FUTURE_SIGHT, SPLASH, RAPID_SPIN, ICE_BALL],
    MANTINE => [TWISTER, HYDRO_PUMP, HAZE, SLAM, MUD_SPORT, ROCK_SLIDE],
    SKARMORY => [DRILL_PECK, PURSUIT, WHIRLWIND, SKY_ATTACK, CURSE],
    HOUNDOUR => [FIRE_SPIN, RAGE, PURSUIT, COUNTER, SPITE, REVERSAL, BEAT_UP, WILL_O_WISP],
    PHANPY => [FOCUS_ENERGY, BODY_SLAM, ANCIENT_POWER, SNORE, COUNTER, FISSURE],
    STANTLER => [SPITE, DISABLE, BITE, SWAGGER, PSYCH_UP, EXTRASENSORY],
    TYROGUE => [RAPID_SPIN, HI_JUMP_KICK, MACH_PUNCH, MIND_READER, HELPING_HAND],
    SMOOCHUM => [MEDITATE, PSYCH_UP, FAKE_OUT, WISH, ICE_PUNCH],
    ELEKID => [KARATE_CHOP, BARRIER, ROLLING_KICK, MEDITATE, CROSS_CHOP, FIRE_PUNCH, ICE_PUNCH],
    MAGBY => [KARATE_CHOP, MEGA_PUNCH, BARRIER, SCREECH, CROSS_CHOP, THUNDER_PUNCH],
    MILTANK => [PRESENT, REVERSAL, SEISMIC_TOSS, ENDURE, PSYCH_UP, CURSE, HELPING_HAND, SLEEP_TALK],
    LARVITAR => [PURSUIT, STOMP, OUTRAGE, FOCUS_ENERGY, ANCIENT_POWER, DRAGON_DANCE, CURSE],
    TREECKO => [CRUNCH, MUD_SPORT, ENDEAVOR, LEECH_SEED, DRAGON_BREATH, CRUSH_CLAW],
    TORCHIC => [COUNTER, REVERSAL, ENDURE, SWAGGER, ROCK_SLIDE, SMELLING_SALT],
    MUDKIP => [REFRESH, UPROAR, CURSE, STOMP, ICE_BALL, MIRROR_COAT],
    POOCHYENA => [ASTONISH, POISON_FANG, COVET, LEER, YAWN],
    ZIGZAGOON => [CHARM, PURSUIT, SUBSTITUTE, TICKLE, TRICK],
    LOTAD => [SYNTHESIS, RAZOR_LEAF, SWEET_SCENT, LEECH_SEED, FLAIL, WATER_GUN],
    SEEDOT => [LEECH_SEED, AMNESIA, QUICK_ATTACK, RAZOR_WIND, TAKE_DOWN, FALSE_SWIPE],
    NINCADA => [ENDURE, FAINT_ATTACK, GUST, SILVER_WIND],
    TAILLOW => [PURSUIT, SUPERSONIC, REFRESH, MIRROR_MOVE, RAGE, SKY_ATTACK],
    SHROOMISH => [FAKE_TEARS, SWAGGER, CHARM, FALSE_SWIPE, HELPING_HAND],
    SPINDA => [ENCORE, ROCK_SLIDE, ASSIST, DISABLE, BATON_PASS, WISH, TRICK, SMELLING_SALT],
    WINGULL => [MIST, TWISTER, AGILITY, GUST, WATER_SPORT],
    SURSKIT => [FORESIGHT, MUD_SHOT, PSYBEAM, HYDRO_PUMP, MIND_READER],
    WAILMER => [DOUBLE_EDGE, THRASH, SWAGGER, SNORE, SLEEP_TALK, CURSE, FISSURE, TICKLE],
    SKITTY => [HELPING_HAND, PSYCH_UP, UPROAR, FAKE_TEARS, WISH, BATON_PASS, SUBSTITUTE, TICKLE],
    KECLEON => [DISABLE, MAGIC_COAT, TRICK],
    NOSEPASS => [MAGNITUDE, ROLLOUT, EXPLOSION],
    TORKOAL => [ERUPTION, ENDURE, SLEEP_TALK, YAWN],
    SABLEYE => [PSYCH_UP, RECOVER, MOONLIGHT],
    BARBOACH => [THRASH, WHIRLPOOL, SPARK],
    LUVDISC => [SPLASH, SUPERSONIC, WATER_SPORT, MUD_SPORT],
    CORPHISH => [MUD_SPORT, ENDEAVOR, BODY_SLAM, ANCIENT_POWER],
    FEEBAS => [MIRROR_COAT, DRAGON_BREATH, MUD_SPORT, HYPNOSIS, LIGHT_SCREEN, CONFUSE_RAY],
    CARVANHA => [HYDRO_PUMP, DOUBLE_EDGE, THRASH],
    TRAPINCH => [FOCUS_ENERGY, QUICK_ATTACK, GUST],
    MAKUHITA => [FAINT_ATTACK, DETECT, FORESIGHT, HELPING_HAND, CROSS_CHOP, REVENGE, DYNAMIC_PUNCH, COUNTER],
    ELECTRIKE => [CRUNCH, HEADBUTT, UPROAR, CURSE, SWIFT],
    NUMEL => [HOWL, SCARY_FACE, BODY_SLAM, ROLLOUT, DEFENSE_CURL, STOMP],
    SPHEAL => [WATER_SPORT, STOCKPILE, SWALLOW, SPIT_UP, YAWN, ROCK_SLIDE, CURSE, FISSURE],
    CACNEA => [GRASS_WHISTLE, ACID, TEETER_DANCE, DYNAMIC_PUNCH, COUNTER],
    SNORUNT => [BLOCK, SPIKES],
    AZURILL => [ENCORE, SING, REFRESH, SLAM, TICKLE],
    SPOINK => [FUTURE_SIGHT, EXTRASENSORY, SUBSTITUTE, TRICK],
    PLUSLE => [SUBSTITUTE, WISH],
    MINUN => [SUBSTITUTE, WISH],
    MAWILE => [SWORDS_DANCE, FALSE_SWIPE, POISON_FANG, PSYCH_UP, ANCIENT_POWER, TICKLE],
    MEDITITE => [FIRE_PUNCH, THUNDER_PUNCH, ICE_PUNCH, FORESIGHT, FAKE_OUT, BATON_PASS, DYNAMIC_PUNCH],
    SWABLU => [AGILITY, HAZE, PURSUIT, RAGE],
    DUSKULL => [IMPRISON, DESTINY_BOND, PAIN_SPLIT, GRUDGE, MEMENTO, FAINT_ATTACK],
    ROSELIA => [SPIKES, SYNTHESIS, PIN_MISSILE, COTTON_SPORE],
    SLAKOTH => [PURSUIT, SLASH, BODY_SLAM, SNORE, CRUSH_CLAW, CURSE, SLEEP_TALK],
    GULPIN => [DREAM_EATER, ACID_ARMOR, SMOG, PAIN_SPLIT],
    TROPIUS => [HEADBUTT, SLAM, RAZOR_WIND, LEECH_SEED, NATURE_POWER],
    WHISMUR => [TAKE_DOWN, SNORE, SWAGGER, EXTRASENSORY, SMELLING_SALT],
    CLAMPERL => [REFRESH, MUD_SPORT, BODY_SLAM, SUPERSONIC, BARRIER, CONFUSE_RAY],
    ABSOL => [BATON_PASS, FAINT_ATTACK, DOUBLE_EDGE, MAGIC_COAT, CURSE, SUBSTITUTE],
    SHUPPET => [DISABLE, DESTINY_BOND, FORESIGHT, ASTONISH, IMPRISON],
    SEVIPER => [STOCKPILE, SWALLOW, SPIT_UP, BODY_SLAM],
    ZANGOOSE => [FLAIL, DOUBLE_KICK, RAZOR_WIND, COUNTER, ROAR, CURSE],
    RELICANTH => [MAGNITUDE, SKULL_BASH, WATER_SPORT, AMNESIA, SLEEP_TALK, ROCK_SLIDE],
    ARON => [ENDEAVOR, BODY_SLAM, STOMP, SMELLING_SALT],
    CASTFORM => [FUTURE_SIGHT, PSYCH_UP],
    VOLBEAT => [BATON_PASS, SILVER_WIND, TRICK],
    ILLUMISE => [BATON_PASS, SILVER_WIND, GROWTH],
    LILEEP => [BARRIER, RECOVER, MIRROR_COAT, ROCK_SLIDE],
    ANORITH => [RAPID_SPIN, KNOCK_OFF, SWORDS_DANCE, ROCK_SLIDE],
    RALTS => [DISABLE, WILL_O_WISP, MEAN_LOOK, MEMENTO, DESTINY_BOND],
    BAGON => [HYDRO_PUMP, THRASH, DRAGON_RAGE, TWISTER, DRAGON_DANCE],
    CHIMECHO => [DISABLE, CURSE, HYPNOSIS, DREAM_EATER],
}

/// Number of species with an egg-move list.
pub const EGG_MOVE_SPECIES_COUNT: usize = EGG_MOVES.len();

/// Egg moves indexed by species.
#[derive(Debug, Clone, Copy)]
pub struct EggMoveTable {
    groups: &'static [EggMoveList],
}

impl EggMoveTable {
    /// Returns the embedded egg-move table.
    #[must_use]
    pub const fn new() -> Self {
        Self { groups: EGG_MOVES }
    }

    /// Returns the number of species with an egg-move list.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.groups.len()
    }

    /// Returns whether the table contains no egg-move lists.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The egg-move list for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is out of range.
    /// Returns [`AssetError::NoEggMoves`] if the species has no egg moves.
    pub fn get(&self, species: SpeciesId) -> Result<&EggMoveList, AssetError> {
        if species.index() >= crate::species::SpeciesTable::LEN_U16 {
            return Err(AssetError::UnknownSpecies(species.index()));
        }
        self.groups
            .binary_search_by_key(&species, |group| group.species)
            .map(|index| &self.groups[index])
            .map_err(|_| AssetError::NoEggMoves(species.index()))
    }

    /// The egg moves for `species`, or `None` if it has no egg-move group.
    #[must_use]
    pub fn moves_for(&self, species: SpeciesId) -> Option<&'static [MoveId]> {
        self.get(species).ok().map(EggMoveList::moves)
    }

    /// Iterates over the lists in ascending species order.
    pub fn iter(&self) -> impl Iterator<Item = &EggMoveList> {
        self.groups.iter()
    }
}

impl Default for EggMoveTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EggMoveList, EggMoveTable, EGG_MOVES_SPECIES_OFFSET, EGG_MOVES_TERMINATOR,
        EGG_MOVE_SPECIES_COUNT,
    };
    use crate::battle_moves::MoveId;
    use crate::error::AssetError;
    use crate::species::SpeciesId;

    const EXPECTED_SPECIES_GROUP_COUNT: usize = 165;
    const EXPECTED_MOVE_COUNT: usize = 973;
    const EXPECTED_ENCODED_WORD_COUNT: usize = 1139;
    const EXPECTED_SPECIES_OFFSET: u16 = 20000;
    const EXPECTED_TERMINATOR: u16 = 0xFFFF;

    #[test]
    fn table_contains_every_species_group() {
        let table = EggMoveTable::new();
        assert_eq!(table.len(), EXPECTED_SPECIES_GROUP_COUNT);
        assert_eq!(EGG_MOVE_SPECIES_COUNT, EXPECTED_SPECIES_GROUP_COUNT);
        assert_eq!(table.iter().count(), EXPECTED_SPECIES_GROUP_COUNT);
        assert!(!table.is_empty());
    }

    #[test]
    fn encoded_stream_has_the_expected_word_count() {
        let table = EggMoveTable::new();
        let move_count: usize = table.iter().map(EggMoveList::len).sum();
        let encoded_word_count = table.len() + move_count + 1;

        assert_eq!(move_count, EXPECTED_MOVE_COUNT);
        assert_eq!(encoded_word_count, EXPECTED_ENCODED_WORD_COUNT);
    }

    #[test]
    fn groups_are_non_empty_and_strictly_ordered() {
        let table = EggMoveTable::new();
        for group in table.iter() {
            assert!(
                !group.is_empty(),
                "species {} has no egg moves",
                group.species.index()
            );
        }
        for (earlier, later) in table.iter().zip(table.iter().skip(1)) {
            assert!(earlier.species < later.species);
        }
    }

    #[test]
    fn sentinels_stay_below_the_terminator() {
        let table = EggMoveTable::new();
        for group in table.iter() {
            let sentinel = group.species.index() + EGG_MOVES_SPECIES_OFFSET;
            assert!(
                sentinel < EGG_MOVES_TERMINATOR,
                "sentinel {sentinel} for species {} collides with the terminator",
                group.species.index()
            );
            assert!(sentinel >= EGG_MOVES_SPECIES_OFFSET);
        }
        assert_eq!(EGG_MOVES_TERMINATOR, EXPECTED_TERMINATOR);
        assert_eq!(EGG_MOVES_SPECIES_OFFSET, EXPECTED_SPECIES_OFFSET);
    }

    #[test]
    fn species_without_a_list_have_no_egg_moves() {
        let table = EggMoveTable::new();
        for species in [SpeciesId::NONE, SpeciesId::PIKACHU] {
            assert_eq!(
                table.get(species),
                Err(AssetError::NoEggMoves(species.index()))
            );
            assert_eq!(table.moves_for(species), None);
        }
    }

    #[test]
    fn out_of_range_species_is_an_error() {
        let table = EggMoveTable::new();
        let first_invalid = crate::species::SpeciesTable::LEN_U16;
        let largest_possible = u16::MAX;
        assert_eq!(
            table.get(SpeciesId(first_invalid)),
            Err(AssetError::UnknownSpecies(first_invalid))
        );
        assert_eq!(
            table.get(SpeciesId(largest_possible)),
            Err(AssetError::UnknownSpecies(largest_possible))
        );
        assert_eq!(table.moves_for(SpeciesId(largest_possible)), None);
    }

    #[test]
    fn representative_lists_preserve_move_order() {
        let table = EggMoveTable::new();
        let moves_for = |species| table.get(species).unwrap().moves;

        assert_eq!(
            moves_for(SpeciesId::BULBASAUR),
            &[
                MoveId::LIGHT_SCREEN,
                MoveId::SKULL_BASH,
                MoveId::SAFEGUARD,
                MoveId::CHARM,
                MoveId::PETAL_DANCE,
                MoveId::MAGICAL_LEAF,
                MoveId::GRASS_WHISTLE,
                MoveId::CURSE,
            ]
        );
        assert_eq!(
            moves_for(SpeciesId::CHARMANDER),
            &[
                MoveId::BELLY_DRUM,
                MoveId::ANCIENT_POWER,
                MoveId::ROCK_SLIDE,
                MoveId::BITE,
                MoveId::OUTRAGE,
                MoveId::BEAT_UP,
                MoveId::SWORDS_DANCE,
                MoveId::DRAGON_DANCE,
            ]
        );
        assert_eq!(moves_for(SpeciesId::SUDOWOODO), &[MoveId::SELF_DESTRUCT]);
        assert_eq!(moves_for(SpeciesId::SHUCKLE), &[MoveId::SWEET_SCENT]);
        assert_eq!(
            moves_for(SpeciesId::TREECKO),
            &[
                MoveId::CRUNCH,
                MoveId::MUD_SPORT,
                MoveId::ENDEAVOR,
                MoveId::LEECH_SEED,
                MoveId::DRAGON_BREATH,
                MoveId::CRUSH_CLAW,
            ]
        );
        assert_eq!(
            moves_for(SpeciesId::CHIMECHO),
            &[
                MoveId::DISABLE,
                MoveId::CURSE,
                MoveId::HYPNOSIS,
                MoveId::DREAM_EATER,
            ]
        );
        assert_eq!(table.iter().last().unwrap().species, SpeciesId::CHIMECHO);
    }

    #[test]
    fn teaches_matches_the_move_list() {
        let table = EggMoveTable::new();
        let bulbasaur = table.get(SpeciesId::BULBASAUR).unwrap();
        assert!(bulbasaur.teaches(MoveId::CURSE));
        assert!(!bulbasaur.teaches(MoveId::POUND));
    }
}
