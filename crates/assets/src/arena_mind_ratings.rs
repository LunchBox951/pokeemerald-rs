//! The Battle Arena's per-move "mind" rating.
//!
//! The Arena judges a battler's mind from a signed value added each time it
//! uses a move: attacking moves score +1, Fake Out, Protect, Detect and Endure
//! score -1, and everything else (status moves, plus Counter, Mirror Coat and
//! Bide) scores 0. Only the table is modelled here: nothing accumulates points
//! or draws the judgment screen from these values yet.

use crate::battle_moves::{MoveId, MOVES_COUNT};
use crate::error::AssetError;

/// Rating for moves the table does not list.
const NEUTRAL: i8 = 0;

macro_rules! define_mind_ratings {
    (
        rewarded: [$($up:ident),+ $(,)?],
        penalised: [$($down:ident),+ $(,)?] $(,)?
    ) => {
        const MIND_RATINGS: [i8; MOVES_COUNT] = {
            let mut ratings = [NEUTRAL; MOVES_COUNT];
            $(ratings[MoveId::$up.0 as usize] = 1;)+
            $(ratings[MoveId::$down.0 as usize] = -1;)+
            ratings
        };

        /// Number of moves rated +1.
        pub const MIND_REWARDED_MOVE_COUNT: usize = [$(MoveId::$up),+].len();
        /// Number of moves rated -1.
        pub const MIND_PENALISED_MOVE_COUNT: usize = [$(MoveId::$down),+].len();
    };
}

#[rustfmt::skip]
define_mind_ratings! {
    rewarded: [
    POUND,
    KARATE_CHOP,
    DOUBLE_SLAP,
    COMET_PUNCH,
    MEGA_PUNCH,
    PAY_DAY,
    FIRE_PUNCH,
    ICE_PUNCH,
    THUNDER_PUNCH,
    SCRATCH,
    VICE_GRIP,
    GUILLOTINE,
    RAZOR_WIND,
    CUT,
    GUST,
    WING_ATTACK,
    FLY,
    BIND,
    SLAM,
    VINE_WHIP,
    STOMP,
    DOUBLE_KICK,
    MEGA_KICK,
    JUMP_KICK,
    ROLLING_KICK,
    HEADBUTT,
    HORN_ATTACK,
    FURY_ATTACK,
    HORN_DRILL,
    TACKLE,
    BODY_SLAM,
    WRAP,
    TAKE_DOWN,
    THRASH,
    DOUBLE_EDGE,
    POISON_STING,
    TWINEEDLE,
    PIN_MISSILE,
    BITE,
    SONIC_BOOM,
    ACID,
    EMBER,
    FLAMETHROWER,
    WATER_GUN,
    HYDRO_PUMP,
    SURF,
    ICE_BEAM,
    BLIZZARD,
    PSYBEAM,
    BUBBLE_BEAM,
    AURORA_BEAM,
    HYPER_BEAM,
    PECK,
    DRILL_PECK,
    SUBMISSION,
    LOW_KICK,
    SEISMIC_TOSS,
    STRENGTH,
    ABSORB,
    MEGA_DRAIN,
    RAZOR_LEAF,
    SOLAR_BEAM,
    PETAL_DANCE,
    DRAGON_RAGE,
    FIRE_SPIN,
    THUNDER_SHOCK,
    THUNDERBOLT,
    THUNDER,
    ROCK_THROW,
    EARTHQUAKE,
    FISSURE,
    DIG,
    CONFUSION,
    PSYCHIC,
    QUICK_ATTACK,
    RAGE,
    NIGHT_SHADE,
    SELF_DESTRUCT,
    EGG_BOMB,
    LICK,
    SMOG,
    SLUDGE,
    BONE_CLUB,
    FIRE_BLAST,
    WATERFALL,
    CLAMP,
    SWIFT,
    SKULL_BASH,
    SPIKE_CANNON,
    CONSTRICT,
    HI_JUMP_KICK,
    DREAM_EATER,
    BARRAGE,
    LEECH_LIFE,
    SKY_ATTACK,
    BUBBLE,
    DIZZY_PUNCH,
    PSYWAVE,
    CRABHAMMER,
    EXPLOSION,
    FURY_SWIPES,
    BONEMERANG,
    ROCK_SLIDE,
    HYPER_FANG,
    TRI_ATTACK,
    SUPER_FANG,
    SLASH,
    STRUGGLE,
    TRIPLE_KICK,
    THIEF,
    FLAME_WHEEL,
    SNORE,
    FLAIL,
    AEROBLAST,
    REVERSAL,
    POWDER_SNOW,
    MACH_PUNCH,
    FAINT_ATTACK,
    SLUDGE_BOMB,
    MUD_SLAP,
    OCTAZOOKA,
    ZAP_CANNON,
    ICY_WIND,
    BONE_RUSH,
    OUTRAGE,
    GIGA_DRAIN,
    ROLLOUT,
    FALSE_SWIPE,
    SPARK,
    FURY_CUTTER,
    STEEL_WING,
    RETURN,
    PRESENT,
    FRUSTRATION,
    SACRED_FIRE,
    MAGNITUDE,
    DYNAMIC_PUNCH,
    MEGAHORN,
    DRAGON_BREATH,
    PURSUIT,
    RAPID_SPIN,
    IRON_TAIL,
    METAL_CLAW,
    VITAL_THROW,
    HIDDEN_POWER,
    CROSS_CHOP,
    TWISTER,
    CRUNCH,
    EXTREME_SPEED,
    ANCIENT_POWER,
    SHADOW_BALL,
    FUTURE_SIGHT,
    ROCK_SMASH,
    WHIRLPOOL,
    BEAT_UP,
    UPROAR,
    SPIT_UP,
    HEAT_WAVE,
    FACADE,
    FOCUS_PUNCH,
    SMELLING_SALT,
    SUPERPOWER,
    REVENGE,
    BRICK_BREAK,
    KNOCK_OFF,
    ENDEAVOR,
    ERUPTION,
    SECRET_POWER,
    DIVE,
    ARM_THRUST,
    LUSTER_PURGE,
    MIST_BALL,
    BLAZE_KICK,
    ICE_BALL,
    NEEDLE_ARM,
    HYPER_VOICE,
    POISON_FANG,
    CRUSH_CLAW,
    BLAST_BURN,
    HYDRO_CANNON,
    METEOR_MASH,
    ASTONISH,
    WEATHER_BALL,
    AIR_CUTTER,
    OVERHEAT,
    ROCK_TOMB,
    SILVER_WIND,
    WATER_SPOUT,
    SIGNAL_BEAM,
    SHADOW_PUNCH,
    EXTRASENSORY,
    SKY_UPPERCUT,
    SAND_TOMB,
    SHEER_COLD,
    MUDDY_WATER,
    BULLET_SEED,
    AERIAL_ACE,
    ICICLE_SPEAR,
    DRAGON_CLAW,
    FRENZY_PLANT,
    BOUNCE,
    MUD_SHOT,
    POISON_TAIL,
    COVET,
    VOLT_TACKLE,
    MAGICAL_LEAF,
    LEAF_BLADE,
    ROCK_BLAST,
    SHOCK_WAVE,
    WATER_PULSE,
    DOOM_DESIRE,
    PSYCHO_BOOST,
    ],
    penalised: [
    PROTECT,
    DETECT,
    ENDURE,
    FAKE_OUT,
    ],
}

/// Returns the mind points the Battle Arena adds when a battler uses `move_id`:
/// +1, -1, or 0 for every move the table does not list.
///
/// # Errors
///
/// Returns [`AssetError::UnknownMove`] when `move_id` is outside the move
/// table.
pub fn arena_mind_rating(move_id: MoveId) -> Result<i8, AssetError> {
    MIND_RATINGS
        .get(usize::from(move_id.index()))
        .copied()
        .ok_or(AssetError::UnknownMove(move_id.index()))
}

#[cfg(test)]
mod tests {
    use super::{arena_mind_rating, MIND_PENALISED_MOVE_COUNT, MIND_REWARDED_MOVE_COUNT};
    use crate::battle_moves::{MoveId, MOVES_COUNT};
    use crate::error::AssetError;

    /// Every move rated +1, as a move index, ordered by index.
    const REWARDED: [u16; 212] = [
        1,   // POUND
        2,   // KARATE_CHOP
        3,   // DOUBLE_SLAP
        4,   // COMET_PUNCH
        5,   // MEGA_PUNCH
        6,   // PAY_DAY
        7,   // FIRE_PUNCH
        8,   // ICE_PUNCH
        9,   // THUNDER_PUNCH
        10,  // SCRATCH
        11,  // VICE_GRIP
        12,  // GUILLOTINE
        13,  // RAZOR_WIND
        15,  // CUT
        16,  // GUST
        17,  // WING_ATTACK
        19,  // FLY
        20,  // BIND
        21,  // SLAM
        22,  // VINE_WHIP
        23,  // STOMP
        24,  // DOUBLE_KICK
        25,  // MEGA_KICK
        26,  // JUMP_KICK
        27,  // ROLLING_KICK
        29,  // HEADBUTT
        30,  // HORN_ATTACK
        31,  // FURY_ATTACK
        32,  // HORN_DRILL
        33,  // TACKLE
        34,  // BODY_SLAM
        35,  // WRAP
        36,  // TAKE_DOWN
        37,  // THRASH
        38,  // DOUBLE_EDGE
        40,  // POISON_STING
        41,  // TWINEEDLE
        42,  // PIN_MISSILE
        44,  // BITE
        49,  // SONIC_BOOM
        51,  // ACID
        52,  // EMBER
        53,  // FLAMETHROWER
        55,  // WATER_GUN
        56,  // HYDRO_PUMP
        57,  // SURF
        58,  // ICE_BEAM
        59,  // BLIZZARD
        60,  // PSYBEAM
        61,  // BUBBLE_BEAM
        62,  // AURORA_BEAM
        63,  // HYPER_BEAM
        64,  // PECK
        65,  // DRILL_PECK
        66,  // SUBMISSION
        67,  // LOW_KICK
        69,  // SEISMIC_TOSS
        70,  // STRENGTH
        71,  // ABSORB
        72,  // MEGA_DRAIN
        75,  // RAZOR_LEAF
        76,  // SOLAR_BEAM
        80,  // PETAL_DANCE
        82,  // DRAGON_RAGE
        83,  // FIRE_SPIN
        84,  // THUNDER_SHOCK
        85,  // THUNDERBOLT
        87,  // THUNDER
        88,  // ROCK_THROW
        89,  // EARTHQUAKE
        90,  // FISSURE
        91,  // DIG
        93,  // CONFUSION
        94,  // PSYCHIC
        98,  // QUICK_ATTACK
        99,  // RAGE
        101, // NIGHT_SHADE
        120, // SELF_DESTRUCT
        121, // EGG_BOMB
        122, // LICK
        123, // SMOG
        124, // SLUDGE
        125, // BONE_CLUB
        126, // FIRE_BLAST
        127, // WATERFALL
        128, // CLAMP
        129, // SWIFT
        130, // SKULL_BASH
        131, // SPIKE_CANNON
        132, // CONSTRICT
        136, // HI_JUMP_KICK
        138, // DREAM_EATER
        140, // BARRAGE
        141, // LEECH_LIFE
        143, // SKY_ATTACK
        145, // BUBBLE
        146, // DIZZY_PUNCH
        149, // PSYWAVE
        152, // CRABHAMMER
        153, // EXPLOSION
        154, // FURY_SWIPES
        155, // BONEMERANG
        157, // ROCK_SLIDE
        158, // HYPER_FANG
        161, // TRI_ATTACK
        162, // SUPER_FANG
        163, // SLASH
        165, // STRUGGLE
        167, // TRIPLE_KICK
        168, // THIEF
        172, // FLAME_WHEEL
        173, // SNORE
        175, // FLAIL
        177, // AEROBLAST
        179, // REVERSAL
        181, // POWDER_SNOW
        183, // MACH_PUNCH
        185, // FAINT_ATTACK
        188, // SLUDGE_BOMB
        189, // MUD_SLAP
        190, // OCTAZOOKA
        192, // ZAP_CANNON
        196, // ICY_WIND
        198, // BONE_RUSH
        200, // OUTRAGE
        202, // GIGA_DRAIN
        205, // ROLLOUT
        206, // FALSE_SWIPE
        209, // SPARK
        210, // FURY_CUTTER
        211, // STEEL_WING
        216, // RETURN
        217, // PRESENT
        218, // FRUSTRATION
        221, // SACRED_FIRE
        222, // MAGNITUDE
        223, // DYNAMIC_PUNCH
        224, // MEGAHORN
        225, // DRAGON_BREATH
        228, // PURSUIT
        229, // RAPID_SPIN
        231, // IRON_TAIL
        232, // METAL_CLAW
        233, // VITAL_THROW
        237, // HIDDEN_POWER
        238, // CROSS_CHOP
        239, // TWISTER
        242, // CRUNCH
        245, // EXTREME_SPEED
        246, // ANCIENT_POWER
        247, // SHADOW_BALL
        248, // FUTURE_SIGHT
        249, // ROCK_SMASH
        250, // WHIRLPOOL
        251, // BEAT_UP
        253, // UPROAR
        255, // SPIT_UP
        257, // HEAT_WAVE
        263, // FACADE
        264, // FOCUS_PUNCH
        265, // SMELLING_SALT
        276, // SUPERPOWER
        279, // REVENGE
        280, // BRICK_BREAK
        282, // KNOCK_OFF
        283, // ENDEAVOR
        284, // ERUPTION
        290, // SECRET_POWER
        291, // DIVE
        292, // ARM_THRUST
        295, // LUSTER_PURGE
        296, // MIST_BALL
        299, // BLAZE_KICK
        301, // ICE_BALL
        302, // NEEDLE_ARM
        304, // HYPER_VOICE
        305, // POISON_FANG
        306, // CRUSH_CLAW
        307, // BLAST_BURN
        308, // HYDRO_CANNON
        309, // METEOR_MASH
        310, // ASTONISH
        311, // WEATHER_BALL
        314, // AIR_CUTTER
        315, // OVERHEAT
        317, // ROCK_TOMB
        318, // SILVER_WIND
        323, // WATER_SPOUT
        324, // SIGNAL_BEAM
        325, // SHADOW_PUNCH
        326, // EXTRASENSORY
        327, // SKY_UPPERCUT
        328, // SAND_TOMB
        329, // SHEER_COLD
        330, // MUDDY_WATER
        331, // BULLET_SEED
        332, // AERIAL_ACE
        333, // ICICLE_SPEAR
        337, // DRAGON_CLAW
        338, // FRENZY_PLANT
        340, // BOUNCE
        341, // MUD_SHOT
        342, // POISON_TAIL
        343, // COVET
        344, // VOLT_TACKLE
        345, // MAGICAL_LEAF
        348, // LEAF_BLADE
        350, // ROCK_BLAST
        351, // SHOCK_WAVE
        352, // WATER_PULSE
        353, // DOOM_DESIRE
        354, // PSYCHO_BOOST
    ];

    /// Every move rated -1, as (move index, rating), ordered by index.
    const PENALISED: [(u16, i8); 4] = [
        (182, -1), // PROTECT
        (197, -1), // DETECT
        (203, -1), // ENDURE
        (252, -1), // FAKE_OUT
    ];

    fn rating(index: u16) -> Result<i8, AssetError> {
        arena_mind_rating(MoveId(index))
    }

    #[test]
    fn listed_move_counts_match_the_table() {
        assert_eq!(MIND_REWARDED_MOVE_COUNT, 212);
        assert_eq!(MIND_PENALISED_MOVE_COUNT, 4);
        assert_eq!(REWARDED.len(), 212);
        assert_eq!(PENALISED.len(), 4);
    }

    #[test]
    fn every_rewarded_move_scores_plus_one() {
        for index in REWARDED {
            assert_eq!(rating(index), Ok(1), "move {index}");
        }
    }

    #[test]
    fn every_penalised_move_scores_minus_one() {
        for (index, expected) in PENALISED {
            assert_eq!(rating(index), Ok(expected), "move {index}");
        }
    }

    #[test]
    fn protect_detect_endure_and_fake_out_are_penalised() {
        assert_eq!(arena_mind_rating(MoveId::PROTECT), Ok(-1));
        assert_eq!(arena_mind_rating(MoveId::DETECT), Ok(-1));
        assert_eq!(arena_mind_rating(MoveId::ENDURE), Ok(-1));
        assert_eq!(arena_mind_rating(MoveId::FAKE_OUT), Ok(-1));
    }

    #[test]
    fn counter_mirror_coat_and_bide_are_neutral() {
        assert_eq!(arena_mind_rating(MoveId::COUNTER), Ok(0));
        assert_eq!(arena_mind_rating(MoveId::MIRROR_COAT), Ok(0));
        assert_eq!(arena_mind_rating(MoveId::BIDE), Ok(0));
    }

    #[test]
    fn every_other_in_range_move_is_neutral() {
        let mut neutral = 0_usize;
        for index in 0..u16::try_from(MOVES_COUNT).expect("move count fits u16") {
            let listed = REWARDED.contains(&index) || PENALISED.iter().any(|&(m, _)| m == index);
            if !listed {
                assert_eq!(rating(index), Ok(0), "move {index}");
                neutral += 1;
            }
        }
        assert_eq!(neutral, MOVES_COUNT - 212 - 4);
    }

    #[test]
    fn none_is_neutral() {
        assert_eq!(arena_mind_rating(MoveId::NONE), Ok(0));
    }

    #[test]
    fn out_of_range_moves_are_refused() {
        let first_invalid = u16::try_from(MOVES_COUNT).expect("move count fits u16");
        assert_eq!(
            rating(first_invalid),
            Err(AssetError::UnknownMove(first_invalid))
        );
        assert_eq!(rating(u16::MAX), Err(AssetError::UnknownMove(u16::MAX)));
    }
}
