//! Display names for every move.

use crate::battle_moves::MoveId;
use crate::error::AssetError;
use crate::MOVES_COUNT;

/// Provides lookup over the canonical move display names.
#[derive(Debug, Clone, Copy)]
pub struct MoveNames {
    names: &'static [&'static str],
}

impl MoveNames {
    /// The number of addressable move IDs, including the empty sentinel.
    pub const LEN: usize = NAMES.len();

    /// Builds the canonical move-name table.
    #[must_use]
    pub const fn new() -> Self {
        Self { names: &NAMES }
    }

    /// Returns the display name stored at `move_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMove`] when `move_id` is outside the table.
    pub fn name(&self, move_id: MoveId) -> Result<&'static str, AssetError> {
        self.names
            .get(usize::from(move_id.index()))
            .copied()
            .ok_or(AssetError::UnknownMove(move_id.index()))
    }

    /// Returns the number of move-name slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns `false`; the canonical move-name table is never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl Default for MoveNames {
    fn default() -> Self {
        Self::new()
    }
}

const fn ordered_names(
    entries: &[(MoveId, &'static str); MOVES_COUNT],
) -> [&'static str; MOVES_COUNT] {
    let mut names = [""; MOVES_COUNT];
    let mut index = 0;
    while index < MOVES_COUNT {
        let (move_id, name) = entries[index];
        assert!(
            move_id.index() as usize == index,
            "move names must use ascending MoveId order"
        );
        names[index] = name;
        index += 1;
    }
    names
}

const NAMES: [&str; MOVES_COUNT] = ordered_names(&[
    (MoveId::NONE, "-"),
    (MoveId::POUND, "POUND"),
    (MoveId::KARATE_CHOP, "KARATE CHOP"),
    (MoveId::DOUBLE_SLAP, "DOUBLESLAP"),
    (MoveId::COMET_PUNCH, "COMET PUNCH"),
    (MoveId::MEGA_PUNCH, "MEGA PUNCH"),
    (MoveId::PAY_DAY, "PAY DAY"),
    (MoveId::FIRE_PUNCH, "FIRE PUNCH"),
    (MoveId::ICE_PUNCH, "ICE PUNCH"),
    (MoveId::THUNDER_PUNCH, "THUNDERPUNCH"),
    (MoveId::SCRATCH, "SCRATCH"),
    (MoveId::VICE_GRIP, "VICEGRIP"),
    (MoveId::GUILLOTINE, "GUILLOTINE"),
    (MoveId::RAZOR_WIND, "RAZOR WIND"),
    (MoveId::SWORDS_DANCE, "SWORDS DANCE"),
    (MoveId::CUT, "CUT"),
    (MoveId::GUST, "GUST"),
    (MoveId::WING_ATTACK, "WING ATTACK"),
    (MoveId::WHIRLWIND, "WHIRLWIND"),
    (MoveId::FLY, "FLY"),
    (MoveId::BIND, "BIND"),
    (MoveId::SLAM, "SLAM"),
    (MoveId::VINE_WHIP, "VINE WHIP"),
    (MoveId::STOMP, "STOMP"),
    (MoveId::DOUBLE_KICK, "DOUBLE KICK"),
    (MoveId::MEGA_KICK, "MEGA KICK"),
    (MoveId::JUMP_KICK, "JUMP KICK"),
    (MoveId::ROLLING_KICK, "ROLLING KICK"),
    (MoveId::SAND_ATTACK, "SAND-ATTACK"),
    (MoveId::HEADBUTT, "HEADBUTT"),
    (MoveId::HORN_ATTACK, "HORN ATTACK"),
    (MoveId::FURY_ATTACK, "FURY ATTACK"),
    (MoveId::HORN_DRILL, "HORN DRILL"),
    (MoveId::TACKLE, "TACKLE"),
    (MoveId::BODY_SLAM, "BODY SLAM"),
    (MoveId::WRAP, "WRAP"),
    (MoveId::TAKE_DOWN, "TAKE DOWN"),
    (MoveId::THRASH, "THRASH"),
    (MoveId::DOUBLE_EDGE, "DOUBLE-EDGE"),
    (MoveId::TAIL_WHIP, "TAIL WHIP"),
    (MoveId::POISON_STING, "POISON STING"),
    (MoveId::TWINEEDLE, "TWINEEDLE"),
    (MoveId::PIN_MISSILE, "PIN MISSILE"),
    (MoveId::LEER, "LEER"),
    (MoveId::BITE, "BITE"),
    (MoveId::GROWL, "GROWL"),
    (MoveId::ROAR, "ROAR"),
    (MoveId::SING, "SING"),
    (MoveId::SUPERSONIC, "SUPERSONIC"),
    (MoveId::SONIC_BOOM, "SONICBOOM"),
    (MoveId::DISABLE, "DISABLE"),
    (MoveId::ACID, "ACID"),
    (MoveId::EMBER, "EMBER"),
    (MoveId::FLAMETHROWER, "FLAMETHROWER"),
    (MoveId::MIST, "MIST"),
    (MoveId::WATER_GUN, "WATER GUN"),
    (MoveId::HYDRO_PUMP, "HYDRO PUMP"),
    (MoveId::SURF, "SURF"),
    (MoveId::ICE_BEAM, "ICE BEAM"),
    (MoveId::BLIZZARD, "BLIZZARD"),
    (MoveId::PSYBEAM, "PSYBEAM"),
    (MoveId::BUBBLE_BEAM, "BUBBLEBEAM"),
    (MoveId::AURORA_BEAM, "AURORA BEAM"),
    (MoveId::HYPER_BEAM, "HYPER BEAM"),
    (MoveId::PECK, "PECK"),
    (MoveId::DRILL_PECK, "DRILL PECK"),
    (MoveId::SUBMISSION, "SUBMISSION"),
    (MoveId::LOW_KICK, "LOW KICK"),
    (MoveId::COUNTER, "COUNTER"),
    (MoveId::SEISMIC_TOSS, "SEISMIC TOSS"),
    (MoveId::STRENGTH, "STRENGTH"),
    (MoveId::ABSORB, "ABSORB"),
    (MoveId::MEGA_DRAIN, "MEGA DRAIN"),
    (MoveId::LEECH_SEED, "LEECH SEED"),
    (MoveId::GROWTH, "GROWTH"),
    (MoveId::RAZOR_LEAF, "RAZOR LEAF"),
    (MoveId::SOLAR_BEAM, "SOLARBEAM"),
    (MoveId::POISON_POWDER, "POISONPOWDER"),
    (MoveId::STUN_SPORE, "STUN SPORE"),
    (MoveId::SLEEP_POWDER, "SLEEP POWDER"),
    (MoveId::PETAL_DANCE, "PETAL DANCE"),
    (MoveId::STRING_SHOT, "STRING SHOT"),
    (MoveId::DRAGON_RAGE, "DRAGON RAGE"),
    (MoveId::FIRE_SPIN, "FIRE SPIN"),
    (MoveId::THUNDER_SHOCK, "THUNDERSHOCK"),
    (MoveId::THUNDERBOLT, "THUNDERBOLT"),
    (MoveId::THUNDER_WAVE, "THUNDER WAVE"),
    (MoveId::THUNDER, "THUNDER"),
    (MoveId::ROCK_THROW, "ROCK THROW"),
    (MoveId::EARTHQUAKE, "EARTHQUAKE"),
    (MoveId::FISSURE, "FISSURE"),
    (MoveId::DIG, "DIG"),
    (MoveId::TOXIC, "TOXIC"),
    (MoveId::CONFUSION, "CONFUSION"),
    (MoveId::PSYCHIC, "PSYCHIC"),
    (MoveId::HYPNOSIS, "HYPNOSIS"),
    (MoveId::MEDITATE, "MEDITATE"),
    (MoveId::AGILITY, "AGILITY"),
    (MoveId::QUICK_ATTACK, "QUICK ATTACK"),
    (MoveId::RAGE, "RAGE"),
    (MoveId::TELEPORT, "TELEPORT"),
    (MoveId::NIGHT_SHADE, "NIGHT SHADE"),
    (MoveId::MIMIC, "MIMIC"),
    (MoveId::SCREECH, "SCREECH"),
    (MoveId::DOUBLE_TEAM, "DOUBLE TEAM"),
    (MoveId::RECOVER, "RECOVER"),
    (MoveId::HARDEN, "HARDEN"),
    (MoveId::MINIMIZE, "MINIMIZE"),
    (MoveId::SMOKESCREEN, "SMOKESCREEN"),
    (MoveId::CONFUSE_RAY, "CONFUSE RAY"),
    (MoveId::WITHDRAW, "WITHDRAW"),
    (MoveId::DEFENSE_CURL, "DEFENSE CURL"),
    (MoveId::BARRIER, "BARRIER"),
    (MoveId::LIGHT_SCREEN, "LIGHT SCREEN"),
    (MoveId::HAZE, "HAZE"),
    (MoveId::REFLECT, "REFLECT"),
    (MoveId::FOCUS_ENERGY, "FOCUS ENERGY"),
    (MoveId::BIDE, "BIDE"),
    (MoveId::METRONOME, "METRONOME"),
    (MoveId::MIRROR_MOVE, "MIRROR MOVE"),
    (MoveId::SELF_DESTRUCT, "SELFDESTRUCT"),
    (MoveId::EGG_BOMB, "EGG BOMB"),
    (MoveId::LICK, "LICK"),
    (MoveId::SMOG, "SMOG"),
    (MoveId::SLUDGE, "SLUDGE"),
    (MoveId::BONE_CLUB, "BONE CLUB"),
    (MoveId::FIRE_BLAST, "FIRE BLAST"),
    (MoveId::WATERFALL, "WATERFALL"),
    (MoveId::CLAMP, "CLAMP"),
    (MoveId::SWIFT, "SWIFT"),
    (MoveId::SKULL_BASH, "SKULL BASH"),
    (MoveId::SPIKE_CANNON, "SPIKE CANNON"),
    (MoveId::CONSTRICT, "CONSTRICT"),
    (MoveId::AMNESIA, "AMNESIA"),
    (MoveId::KINESIS, "KINESIS"),
    (MoveId::SOFT_BOILED, "SOFTBOILED"),
    (MoveId::HI_JUMP_KICK, "HI JUMP KICK"),
    (MoveId::GLARE, "GLARE"),
    (MoveId::DREAM_EATER, "DREAM EATER"),
    (MoveId::POISON_GAS, "POISON GAS"),
    (MoveId::BARRAGE, "BARRAGE"),
    (MoveId::LEECH_LIFE, "LEECH LIFE"),
    (MoveId::LOVELY_KISS, "LOVELY KISS"),
    (MoveId::SKY_ATTACK, "SKY ATTACK"),
    (MoveId::TRANSFORM, "TRANSFORM"),
    (MoveId::BUBBLE, "BUBBLE"),
    (MoveId::DIZZY_PUNCH, "DIZZY PUNCH"),
    (MoveId::SPORE, "SPORE"),
    (MoveId::FLASH, "FLASH"),
    (MoveId::PSYWAVE, "PSYWAVE"),
    (MoveId::SPLASH, "SPLASH"),
    (MoveId::ACID_ARMOR, "ACID ARMOR"),
    (MoveId::CRABHAMMER, "CRABHAMMER"),
    (MoveId::EXPLOSION, "EXPLOSION"),
    (MoveId::FURY_SWIPES, "FURY SWIPES"),
    (MoveId::BONEMERANG, "BONEMERANG"),
    (MoveId::REST, "REST"),
    (MoveId::ROCK_SLIDE, "ROCK SLIDE"),
    (MoveId::HYPER_FANG, "HYPER FANG"),
    (MoveId::SHARPEN, "SHARPEN"),
    (MoveId::CONVERSION, "CONVERSION"),
    (MoveId::TRI_ATTACK, "TRI ATTACK"),
    (MoveId::SUPER_FANG, "SUPER FANG"),
    (MoveId::SLASH, "SLASH"),
    (MoveId::SUBSTITUTE, "SUBSTITUTE"),
    (MoveId::STRUGGLE, "STRUGGLE"),
    (MoveId::SKETCH, "SKETCH"),
    (MoveId::TRIPLE_KICK, "TRIPLE KICK"),
    (MoveId::THIEF, "THIEF"),
    (MoveId::SPIDER_WEB, "SPIDER WEB"),
    (MoveId::MIND_READER, "MIND READER"),
    (MoveId::NIGHTMARE, "NIGHTMARE"),
    (MoveId::FLAME_WHEEL, "FLAME WHEEL"),
    (MoveId::SNORE, "SNORE"),
    (MoveId::CURSE, "CURSE"),
    (MoveId::FLAIL, "FLAIL"),
    (MoveId::CONVERSION_2, "CONVERSION 2"),
    (MoveId::AEROBLAST, "AEROBLAST"),
    (MoveId::COTTON_SPORE, "COTTON SPORE"),
    (MoveId::REVERSAL, "REVERSAL"),
    (MoveId::SPITE, "SPITE"),
    (MoveId::POWDER_SNOW, "POWDER SNOW"),
    (MoveId::PROTECT, "PROTECT"),
    (MoveId::MACH_PUNCH, "MACH PUNCH"),
    (MoveId::SCARY_FACE, "SCARY FACE"),
    (MoveId::FAINT_ATTACK, "FAINT ATTACK"),
    (MoveId::SWEET_KISS, "SWEET KISS"),
    (MoveId::BELLY_DRUM, "BELLY DRUM"),
    (MoveId::SLUDGE_BOMB, "SLUDGE BOMB"),
    (MoveId::MUD_SLAP, "MUD-SLAP"),
    (MoveId::OCTAZOOKA, "OCTAZOOKA"),
    (MoveId::SPIKES, "SPIKES"),
    (MoveId::ZAP_CANNON, "ZAP CANNON"),
    (MoveId::FORESIGHT, "FORESIGHT"),
    (MoveId::DESTINY_BOND, "DESTINY BOND"),
    (MoveId::PERISH_SONG, "PERISH SONG"),
    (MoveId::ICY_WIND, "ICY WIND"),
    (MoveId::DETECT, "DETECT"),
    (MoveId::BONE_RUSH, "BONE RUSH"),
    (MoveId::LOCK_ON, "LOCK-ON"),
    (MoveId::OUTRAGE, "OUTRAGE"),
    (MoveId::SANDSTORM, "SANDSTORM"),
    (MoveId::GIGA_DRAIN, "GIGA DRAIN"),
    (MoveId::ENDURE, "ENDURE"),
    (MoveId::CHARM, "CHARM"),
    (MoveId::ROLLOUT, "ROLLOUT"),
    (MoveId::FALSE_SWIPE, "FALSE SWIPE"),
    (MoveId::SWAGGER, "SWAGGER"),
    (MoveId::MILK_DRINK, "MILK DRINK"),
    (MoveId::SPARK, "SPARK"),
    (MoveId::FURY_CUTTER, "FURY CUTTER"),
    (MoveId::STEEL_WING, "STEEL WING"),
    (MoveId::MEAN_LOOK, "MEAN LOOK"),
    (MoveId::ATTRACT, "ATTRACT"),
    (MoveId::SLEEP_TALK, "SLEEP TALK"),
    (MoveId::HEAL_BELL, "HEAL BELL"),
    (MoveId::RETURN, "RETURN"),
    (MoveId::PRESENT, "PRESENT"),
    (MoveId::FRUSTRATION, "FRUSTRATION"),
    (MoveId::SAFEGUARD, "SAFEGUARD"),
    (MoveId::PAIN_SPLIT, "PAIN SPLIT"),
    (MoveId::SACRED_FIRE, "SACRED FIRE"),
    (MoveId::MAGNITUDE, "MAGNITUDE"),
    (MoveId::DYNAMIC_PUNCH, "DYNAMICPUNCH"),
    (MoveId::MEGAHORN, "MEGAHORN"),
    (MoveId::DRAGON_BREATH, "DRAGONBREATH"),
    (MoveId::BATON_PASS, "BATON PASS"),
    (MoveId::ENCORE, "ENCORE"),
    (MoveId::PURSUIT, "PURSUIT"),
    (MoveId::RAPID_SPIN, "RAPID SPIN"),
    (MoveId::SWEET_SCENT, "SWEET SCENT"),
    (MoveId::IRON_TAIL, "IRON TAIL"),
    (MoveId::METAL_CLAW, "METAL CLAW"),
    (MoveId::VITAL_THROW, "VITAL THROW"),
    (MoveId::MORNING_SUN, "MORNING SUN"),
    (MoveId::SYNTHESIS, "SYNTHESIS"),
    (MoveId::MOONLIGHT, "MOONLIGHT"),
    (MoveId::HIDDEN_POWER, "HIDDEN POWER"),
    (MoveId::CROSS_CHOP, "CROSS CHOP"),
    (MoveId::TWISTER, "TWISTER"),
    (MoveId::RAIN_DANCE, "RAIN DANCE"),
    (MoveId::SUNNY_DAY, "SUNNY DAY"),
    (MoveId::CRUNCH, "CRUNCH"),
    (MoveId::MIRROR_COAT, "MIRROR COAT"),
    (MoveId::PSYCH_UP, "PSYCH UP"),
    (MoveId::EXTREME_SPEED, "EXTREMESPEED"),
    (MoveId::ANCIENT_POWER, "ANCIENTPOWER"),
    (MoveId::SHADOW_BALL, "SHADOW BALL"),
    (MoveId::FUTURE_SIGHT, "FUTURE SIGHT"),
    (MoveId::ROCK_SMASH, "ROCK SMASH"),
    (MoveId::WHIRLPOOL, "WHIRLPOOL"),
    (MoveId::BEAT_UP, "BEAT UP"),
    (MoveId::FAKE_OUT, "FAKE OUT"),
    (MoveId::UPROAR, "UPROAR"),
    (MoveId::STOCKPILE, "STOCKPILE"),
    (MoveId::SPIT_UP, "SPIT UP"),
    (MoveId::SWALLOW, "SWALLOW"),
    (MoveId::HEAT_WAVE, "HEAT WAVE"),
    (MoveId::HAIL, "HAIL"),
    (MoveId::TORMENT, "TORMENT"),
    (MoveId::FLATTER, "FLATTER"),
    (MoveId::WILL_O_WISP, "WILL-O-WISP"),
    (MoveId::MEMENTO, "MEMENTO"),
    (MoveId::FACADE, "FACADE"),
    (MoveId::FOCUS_PUNCH, "FOCUS PUNCH"),
    (MoveId::SMELLING_SALT, "SMELLINGSALT"),
    (MoveId::FOLLOW_ME, "FOLLOW ME"),
    (MoveId::NATURE_POWER, "NATURE POWER"),
    (MoveId::CHARGE, "CHARGE"),
    (MoveId::TAUNT, "TAUNT"),
    (MoveId::HELPING_HAND, "HELPING HAND"),
    (MoveId::TRICK, "TRICK"),
    (MoveId::ROLE_PLAY, "ROLE PLAY"),
    (MoveId::WISH, "WISH"),
    (MoveId::ASSIST, "ASSIST"),
    (MoveId::INGRAIN, "INGRAIN"),
    (MoveId::SUPERPOWER, "SUPERPOWER"),
    (MoveId::MAGIC_COAT, "MAGIC COAT"),
    (MoveId::RECYCLE, "RECYCLE"),
    (MoveId::REVENGE, "REVENGE"),
    (MoveId::BRICK_BREAK, "BRICK BREAK"),
    (MoveId::YAWN, "YAWN"),
    (MoveId::KNOCK_OFF, "KNOCK OFF"),
    (MoveId::ENDEAVOR, "ENDEAVOR"),
    (MoveId::ERUPTION, "ERUPTION"),
    (MoveId::SKILL_SWAP, "SKILL SWAP"),
    (MoveId::IMPRISON, "IMPRISON"),
    (MoveId::REFRESH, "REFRESH"),
    (MoveId::GRUDGE, "GRUDGE"),
    (MoveId::SNATCH, "SNATCH"),
    (MoveId::SECRET_POWER, "SECRET POWER"),
    (MoveId::DIVE, "DIVE"),
    (MoveId::ARM_THRUST, "ARM THRUST"),
    (MoveId::CAMOUFLAGE, "CAMOUFLAGE"),
    (MoveId::TAIL_GLOW, "TAIL GLOW"),
    (MoveId::LUSTER_PURGE, "LUSTER PURGE"),
    (MoveId::MIST_BALL, "MIST BALL"),
    (MoveId::FEATHER_DANCE, "FEATHERDANCE"),
    (MoveId::TEETER_DANCE, "TEETER DANCE"),
    (MoveId::BLAZE_KICK, "BLAZE KICK"),
    (MoveId::MUD_SPORT, "MUD SPORT"),
    (MoveId::ICE_BALL, "ICE BALL"),
    (MoveId::NEEDLE_ARM, "NEEDLE ARM"),
    (MoveId::SLACK_OFF, "SLACK OFF"),
    (MoveId::HYPER_VOICE, "HYPER VOICE"),
    (MoveId::POISON_FANG, "POISON FANG"),
    (MoveId::CRUSH_CLAW, "CRUSH CLAW"),
    (MoveId::BLAST_BURN, "BLAST BURN"),
    (MoveId::HYDRO_CANNON, "HYDRO CANNON"),
    (MoveId::METEOR_MASH, "METEOR MASH"),
    (MoveId::ASTONISH, "ASTONISH"),
    (MoveId::WEATHER_BALL, "WEATHER BALL"),
    (MoveId::AROMATHERAPY, "AROMATHERAPY"),
    (MoveId::FAKE_TEARS, "FAKE TEARS"),
    (MoveId::AIR_CUTTER, "AIR CUTTER"),
    (MoveId::OVERHEAT, "OVERHEAT"),
    (MoveId::ODOR_SLEUTH, "ODOR SLEUTH"),
    (MoveId::ROCK_TOMB, "ROCK TOMB"),
    (MoveId::SILVER_WIND, "SILVER WIND"),
    (MoveId::METAL_SOUND, "METAL SOUND"),
    (MoveId::GRASS_WHISTLE, "GRASSWHISTLE"),
    (MoveId::TICKLE, "TICKLE"),
    (MoveId::COSMIC_POWER, "COSMIC POWER"),
    (MoveId::WATER_SPOUT, "WATER SPOUT"),
    (MoveId::SIGNAL_BEAM, "SIGNAL BEAM"),
    (MoveId::SHADOW_PUNCH, "SHADOW PUNCH"),
    (MoveId::EXTRASENSORY, "EXTRASENSORY"),
    (MoveId::SKY_UPPERCUT, "SKY UPPERCUT"),
    (MoveId::SAND_TOMB, "SAND TOMB"),
    (MoveId::SHEER_COLD, "SHEER COLD"),
    (MoveId::MUDDY_WATER, "MUDDY WATER"),
    (MoveId::BULLET_SEED, "BULLET SEED"),
    (MoveId::AERIAL_ACE, "AERIAL ACE"),
    (MoveId::ICICLE_SPEAR, "ICICLE SPEAR"),
    (MoveId::IRON_DEFENSE, "IRON DEFENSE"),
    (MoveId::BLOCK, "BLOCK"),
    (MoveId::HOWL, "HOWL"),
    (MoveId::DRAGON_CLAW, "DRAGON CLAW"),
    (MoveId::FRENZY_PLANT, "FRENZY PLANT"),
    (MoveId::BULK_UP, "BULK UP"),
    (MoveId::BOUNCE, "BOUNCE"),
    (MoveId::MUD_SHOT, "MUD SHOT"),
    (MoveId::POISON_TAIL, "POISON TAIL"),
    (MoveId::COVET, "COVET"),
    (MoveId::VOLT_TACKLE, "VOLT TACKLE"),
    (MoveId::MAGICAL_LEAF, "MAGICAL LEAF"),
    (MoveId::WATER_SPORT, "WATER SPORT"),
    (MoveId::CALM_MIND, "CALM MIND"),
    (MoveId::LEAF_BLADE, "LEAF BLADE"),
    (MoveId::DRAGON_DANCE, "DRAGON DANCE"),
    (MoveId::ROCK_BLAST, "ROCK BLAST"),
    (MoveId::SHOCK_WAVE, "SHOCK WAVE"),
    (MoveId::WATER_PULSE, "WATER PULSE"),
    (MoveId::DOOM_DESIRE, "DOOM DESIRE"),
    (MoveId::PSYCHO_BOOST, "PSYCHO BOOST"),
]);

#[cfg(test)]
mod tests {
    use super::{MoveNames, NAMES};
    use crate::battle_moves::MoveId;
    use crate::error::AssetError;
    use crate::MOVES_COUNT;

    #[test]
    fn table_covers_every_move_id() {
        assert_eq!(MoveNames::LEN, MOVES_COUNT);
        assert_eq!(MoveNames::LEN, 355);
        assert_eq!(NAMES.len(), MoveNames::LEN);
        let table = MoveNames::new();
        assert_eq!(table.len(), 355);
        assert!(!table.is_empty());
    }

    #[test]
    fn display_names_match_representative_game_text() {
        let table = MoveNames::new();
        assert_eq!(table.name(MoveId::NONE), Ok("-"));
        assert_eq!(table.name(MoveId::POUND), Ok("POUND"));
        assert_eq!(table.name(MoveId::DOUBLE_SLAP), Ok("DOUBLESLAP"));
        assert_eq!(table.name(MoveId::SAND_ATTACK), Ok("SAND-ATTACK"));
        assert_eq!(table.name(MoveId::DOUBLE_EDGE), Ok("DOUBLE-EDGE"));
        assert_eq!(table.name(MoveId::SONIC_BOOM), Ok("SONICBOOM"));
        assert_eq!(table.name(MoveId::EARTHQUAKE), Ok("EARTHQUAKE"));
        assert_eq!(table.name(MoveId::PSYCHIC), Ok("PSYCHIC"));
        assert_eq!(table.name(MoveId::SHOCK_WAVE), Ok("SHOCK WAVE"));
        assert_eq!(table.name(MoveId::WATER_PULSE), Ok("WATER PULSE"));
        assert_eq!(table.name(MoveId::DOOM_DESIRE), Ok("DOOM DESIRE"));
        assert_eq!(table.name(MoveId::PSYCHO_BOOST), Ok("PSYCHO BOOST"));
    }

    #[test]
    fn out_of_range_move_errors() {
        let table = MoveNames::new();
        let bad = u16::try_from(MoveNames::LEN).unwrap();
        assert_eq!(table.name(MoveId(bad)), Err(AssetError::UnknownMove(bad)));
        assert_eq!(
            table.name(MoveId(u16::MAX)),
            Err(AssetError::UnknownMove(u16::MAX))
        );
    }

    #[test]
    fn names_have_no_duplicates() {
        for (i, a) in NAMES.iter().enumerate() {
            for b in &NAMES[i + 1..] {
                assert_ne!(a, b, "duplicate move name {a:?}");
            }
        }
    }
}
