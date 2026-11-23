//! The Battle Frontier apprentice's valid-move table.
//!
//! The apprentice only asks about, and teaches, moves flagged valid in
//! upstream's per-move table. Upstream notes the criteria are unclear and that
//! a large share of the multi-strike moves are not valid, so eligibility is
//! kept exactly as listed here rather than derived from a move's power,
//! category or hit count: Double Slap is valid while Comet Punch is not, and
//! Aerial Ace is valid while Icicle Spear is not. Only the table is modelled:
//! question generation, dialogue, the apprentice's roster and battles are not.

use crate::battle_moves::{MoveId, MOVES_COUNT};
use crate::error::AssetError;

macro_rules! define_apprentice_moves {
    (
        eligible: [$($yes:ident),+ $(,)?],
        ineligible: [$($no:ident),+ $(,)?] $(,)?
    ) => {
        const VALID_MOVES: [bool; MOVES_COUNT] = {
            // Every move must be listed exactly once, on one side or the other.
            let mut listed = [false; MOVES_COUNT];
            let mut valid = [false; MOVES_COUNT];
            $(
                assert!(!listed[MoveId::$yes.0 as usize], "move listed twice");
                listed[MoveId::$yes.0 as usize] = true;
                valid[MoveId::$yes.0 as usize] = true;
            )+
            $(
                assert!(!listed[MoveId::$no.0 as usize], "move listed twice");
                listed[MoveId::$no.0 as usize] = true;
            )+
            let mut slot = 0;
            while slot < MOVES_COUNT {
                assert!(listed[slot], "move missing from the apprentice table");
                slot += 1;
            }
            valid
        };

        /// Number of moves the apprentice accepts.
        pub const APPRENTICE_VALID_MOVE_COUNT: usize = [$(MoveId::$yes),+].len();
    };
}

#[rustfmt::skip]
define_apprentice_moves! {
    eligible: [
    KARATE_CHOP,
    DOUBLE_SLAP,
    MEGA_PUNCH,
    FIRE_PUNCH,
    ICE_PUNCH,
    THUNDER_PUNCH,
    GUILLOTINE,
    SWORDS_DANCE,
    WHIRLWIND,
    FLY,
    BIND,
    SLAM,
    STOMP,
    DOUBLE_KICK,
    MEGA_KICK,
    JUMP_KICK,
    ROLLING_KICK,
    SAND_ATTACK,
    HEADBUTT,
    HORN_DRILL,
    BODY_SLAM,
    WRAP,
    TAKE_DOWN,
    THRASH,
    DOUBLE_EDGE,
    TWINEEDLE,
    BITE,
    ROAR,
    SING,
    SUPERSONIC,
    SONIC_BOOM,
    DISABLE,
    FLAMETHROWER,
    MIST,
    HYDRO_PUMP,
    SURF,
    ICE_BEAM,
    BLIZZARD,
    PSYBEAM,
    HYPER_BEAM,
    DRILL_PECK,
    SUBMISSION,
    LOW_KICK,
    COUNTER,
    SEISMIC_TOSS,
    STRENGTH,
    LEECH_SEED,
    GROWTH,
    RAZOR_LEAF,
    SOLAR_BEAM,
    POISON_POWDER,
    STUN_SPORE,
    SLEEP_POWDER,
    PETAL_DANCE,
    DRAGON_RAGE,
    FIRE_SPIN,
    THUNDERBOLT,
    THUNDER_WAVE,
    THUNDER,
    EARTHQUAKE,
    FISSURE,
    DIG,
    TOXIC,
    PSYCHIC,
    HYPNOSIS,
    MEDITATE,
    AGILITY,
    QUICK_ATTACK,
    NIGHT_SHADE,
    MIMIC,
    SCREECH,
    DOUBLE_TEAM,
    RECOVER,
    HARDEN,
    MINIMIZE,
    SMOKESCREEN,
    CONFUSE_RAY,
    WITHDRAW,
    DEFENSE_CURL,
    BARRIER,
    LIGHT_SCREEN,
    HAZE,
    REFLECT,
    FOCUS_ENERGY,
    METRONOME,
    MIRROR_MOVE,
    SELF_DESTRUCT,
    EGG_BOMB,
    LICK,
    FIRE_BLAST,
    WATERFALL,
    CLAMP,
    SWIFT,
    SKULL_BASH,
    AMNESIA,
    KINESIS,
    SOFT_BOILED,
    HI_JUMP_KICK,
    GLARE,
    DREAM_EATER,
    LOVELY_KISS,
    SKY_ATTACK,
    TRANSFORM,
    DIZZY_PUNCH,
    SPORE,
    FLASH,
    PSYWAVE,
    ACID_ARMOR,
    CRABHAMMER,
    EXPLOSION,
    BONEMERANG,
    REST,
    ROCK_SLIDE,
    HYPER_FANG,
    SHARPEN,
    CONVERSION,
    TRI_ATTACK,
    SUPER_FANG,
    SLASH,
    SUBSTITUTE,
    STRUGGLE,
    SKETCH,
    TRIPLE_KICK,
    THIEF,
    SPIDER_WEB,
    MIND_READER,
    NIGHTMARE,
    SNORE,
    CURSE,
    FLAIL,
    CONVERSION_2,
    AEROBLAST,
    COTTON_SPORE,
    REVERSAL,
    SPITE,
    PROTECT,
    MACH_PUNCH,
    SCARY_FACE,
    FAINT_ATTACK,
    SWEET_KISS,
    BELLY_DRUM,
    SLUDGE_BOMB,
    MUD_SLAP,
    OCTAZOOKA,
    SPIKES,
    ZAP_CANNON,
    FORESIGHT,
    DESTINY_BOND,
    PERISH_SONG,
    ICY_WIND,
    DETECT,
    LOCK_ON,
    OUTRAGE,
    SANDSTORM,
    GIGA_DRAIN,
    ENDURE,
    CHARM,
    ROLLOUT,
    FALSE_SWIPE,
    SWAGGER,
    MILK_DRINK,
    FURY_CUTTER,
    STEEL_WING,
    MEAN_LOOK,
    ATTRACT,
    SLEEP_TALK,
    HEAL_BELL,
    RETURN,
    PRESENT,
    FRUSTRATION,
    SAFEGUARD,
    PAIN_SPLIT,
    SACRED_FIRE,
    DYNAMIC_PUNCH,
    MEGAHORN,
    DRAGON_BREATH,
    BATON_PASS,
    ENCORE,
    PURSUIT,
    RAPID_SPIN,
    SWEET_SCENT,
    IRON_TAIL,
    METAL_CLAW,
    VITAL_THROW,
    MORNING_SUN,
    SYNTHESIS,
    MOONLIGHT,
    HIDDEN_POWER,
    CROSS_CHOP,
    RAIN_DANCE,
    SUNNY_DAY,
    CRUNCH,
    MIRROR_COAT,
    PSYCH_UP,
    EXTREME_SPEED,
    ANCIENT_POWER,
    SHADOW_BALL,
    FUTURE_SIGHT,
    ROCK_SMASH,
    WHIRLPOOL,
    BEAT_UP,
    FAKE_OUT,
    UPROAR,
    STOCKPILE,
    SPIT_UP,
    SWALLOW,
    HEAT_WAVE,
    HAIL,
    TORMENT,
    FLATTER,
    WILL_O_WISP,
    MEMENTO,
    FACADE,
    FOCUS_PUNCH,
    SMELLING_SALT,
    FOLLOW_ME,
    NATURE_POWER,
    CHARGE,
    TAUNT,
    HELPING_HAND,
    TRICK,
    ROLE_PLAY,
    WISH,
    ASSIST,
    INGRAIN,
    SUPERPOWER,
    MAGIC_COAT,
    RECYCLE,
    REVENGE,
    BRICK_BREAK,
    YAWN,
    KNOCK_OFF,
    ENDEAVOR,
    ERUPTION,
    SKILL_SWAP,
    IMPRISON,
    REFRESH,
    GRUDGE,
    SNATCH,
    SECRET_POWER,
    DIVE,
    CAMOUFLAGE,
    TAIL_GLOW,
    LUSTER_PURGE,
    MIST_BALL,
    FEATHER_DANCE,
    TEETER_DANCE,
    BLAZE_KICK,
    MUD_SPORT,
    NEEDLE_ARM,
    SLACK_OFF,
    HYPER_VOICE,
    CRUSH_CLAW,
    BLAST_BURN,
    HYDRO_CANNON,
    METEOR_MASH,
    ASTONISH,
    WEATHER_BALL,
    AROMATHERAPY,
    FAKE_TEARS,
    AIR_CUTTER,
    OVERHEAT,
    ODOR_SLEUTH,
    ROCK_TOMB,
    SILVER_WIND,
    METAL_SOUND,
    GRASS_WHISTLE,
    TICKLE,
    COSMIC_POWER,
    WATER_SPOUT,
    SIGNAL_BEAM,
    SHADOW_PUNCH,
    EXTRASENSORY,
    SKY_UPPERCUT,
    SAND_TOMB,
    SHEER_COLD,
    MUDDY_WATER,
    AERIAL_ACE,
    IRON_DEFENSE,
    BLOCK,
    HOWL,
    DRAGON_CLAW,
    FRENZY_PLANT,
    BULK_UP,
    BOUNCE,
    POISON_TAIL,
    COVET,
    VOLT_TACKLE,
    MAGICAL_LEAF,
    WATER_SPORT,
    CALM_MIND,
    LEAF_BLADE,
    DRAGON_DANCE,
    SHOCK_WAVE,
    WATER_PULSE,
    DOOM_DESIRE,
    PSYCHO_BOOST,
    ],
    ineligible: [
    NONE,
    POUND,
    COMET_PUNCH,
    PAY_DAY,
    SCRATCH,
    VICE_GRIP,
    RAZOR_WIND,
    CUT,
    GUST,
    WING_ATTACK,
    VINE_WHIP,
    HORN_ATTACK,
    FURY_ATTACK,
    TACKLE,
    TAIL_WHIP,
    POISON_STING,
    PIN_MISSILE,
    LEER,
    GROWL,
    ACID,
    EMBER,
    WATER_GUN,
    BUBBLE_BEAM,
    AURORA_BEAM,
    PECK,
    ABSORB,
    MEGA_DRAIN,
    STRING_SHOT,
    THUNDER_SHOCK,
    ROCK_THROW,
    CONFUSION,
    RAGE,
    TELEPORT,
    BIDE,
    SMOG,
    SLUDGE,
    BONE_CLUB,
    SPIKE_CANNON,
    CONSTRICT,
    POISON_GAS,
    BARRAGE,
    LEECH_LIFE,
    BUBBLE,
    SPLASH,
    FURY_SWIPES,
    FLAME_WHEEL,
    POWDER_SNOW,
    BONE_RUSH,
    SPARK,
    MAGNITUDE,
    TWISTER,
    ARM_THRUST,
    ICE_BALL,
    POISON_FANG,
    BULLET_SEED,
    ICICLE_SPEAR,
    MUD_SHOT,
    ROCK_BLAST,
    ],
}

/// Returns whether the apprentice accepts `move_id`.
///
/// `MoveId::NONE` is a valid table slot and is not accepted.
///
/// # Errors
///
/// Returns [`AssetError::UnknownMove`] when `move_id` is outside the move
/// table; that is distinct from `Ok(false)`, a real move the apprentice does
/// not accept.
pub fn is_valid_apprentice_move(move_id: MoveId) -> Result<bool, AssetError> {
    VALID_MOVES
        .get(usize::from(move_id.index()))
        .copied()
        .ok_or(AssetError::UnknownMove(move_id.index()))
}

#[cfg(test)]
mod tests {
    use super::{is_valid_apprentice_move, APPRENTICE_VALID_MOVE_COUNT};
    use crate::battle_moves::{MoveId, MOVES_COUNT};
    use crate::error::AssetError;

    /// Upstream's value for every move slot, in move-index order.
    #[rustfmt::skip]
    const EXPECTED: [bool; MOVES_COUNT] = [
        false, // 0 NONE
        false, // 1 POUND
        true, // 2 KARATE_CHOP
        true, // 3 DOUBLE_SLAP
        false, // 4 COMET_PUNCH
        true, // 5 MEGA_PUNCH
        false, // 6 PAY_DAY
        true, // 7 FIRE_PUNCH
        true, // 8 ICE_PUNCH
        true, // 9 THUNDER_PUNCH
        false, // 10 SCRATCH
        false, // 11 VICE_GRIP
        true, // 12 GUILLOTINE
        false, // 13 RAZOR_WIND
        true, // 14 SWORDS_DANCE
        false, // 15 CUT
        false, // 16 GUST
        false, // 17 WING_ATTACK
        true, // 18 WHIRLWIND
        true, // 19 FLY
        true, // 20 BIND
        true, // 21 SLAM
        false, // 22 VINE_WHIP
        true, // 23 STOMP
        true, // 24 DOUBLE_KICK
        true, // 25 MEGA_KICK
        true, // 26 JUMP_KICK
        true, // 27 ROLLING_KICK
        true, // 28 SAND_ATTACK
        true, // 29 HEADBUTT
        false, // 30 HORN_ATTACK
        false, // 31 FURY_ATTACK
        true, // 32 HORN_DRILL
        false, // 33 TACKLE
        true, // 34 BODY_SLAM
        true, // 35 WRAP
        true, // 36 TAKE_DOWN
        true, // 37 THRASH
        true, // 38 DOUBLE_EDGE
        false, // 39 TAIL_WHIP
        false, // 40 POISON_STING
        true, // 41 TWINEEDLE
        false, // 42 PIN_MISSILE
        false, // 43 LEER
        true, // 44 BITE
        false, // 45 GROWL
        true, // 46 ROAR
        true, // 47 SING
        true, // 48 SUPERSONIC
        true, // 49 SONIC_BOOM
        true, // 50 DISABLE
        false, // 51 ACID
        false, // 52 EMBER
        true, // 53 FLAMETHROWER
        true, // 54 MIST
        false, // 55 WATER_GUN
        true, // 56 HYDRO_PUMP
        true, // 57 SURF
        true, // 58 ICE_BEAM
        true, // 59 BLIZZARD
        true, // 60 PSYBEAM
        false, // 61 BUBBLE_BEAM
        false, // 62 AURORA_BEAM
        true, // 63 HYPER_BEAM
        false, // 64 PECK
        true, // 65 DRILL_PECK
        true, // 66 SUBMISSION
        true, // 67 LOW_KICK
        true, // 68 COUNTER
        true, // 69 SEISMIC_TOSS
        true, // 70 STRENGTH
        false, // 71 ABSORB
        false, // 72 MEGA_DRAIN
        true, // 73 LEECH_SEED
        true, // 74 GROWTH
        true, // 75 RAZOR_LEAF
        true, // 76 SOLAR_BEAM
        true, // 77 POISON_POWDER
        true, // 78 STUN_SPORE
        true, // 79 SLEEP_POWDER
        true, // 80 PETAL_DANCE
        false, // 81 STRING_SHOT
        true, // 82 DRAGON_RAGE
        true, // 83 FIRE_SPIN
        false, // 84 THUNDER_SHOCK
        true, // 85 THUNDERBOLT
        true, // 86 THUNDER_WAVE
        true, // 87 THUNDER
        false, // 88 ROCK_THROW
        true, // 89 EARTHQUAKE
        true, // 90 FISSURE
        true, // 91 DIG
        true, // 92 TOXIC
        false, // 93 CONFUSION
        true, // 94 PSYCHIC
        true, // 95 HYPNOSIS
        true, // 96 MEDITATE
        true, // 97 AGILITY
        true, // 98 QUICK_ATTACK
        false, // 99 RAGE
        false, // 100 TELEPORT
        true, // 101 NIGHT_SHADE
        true, // 102 MIMIC
        true, // 103 SCREECH
        true, // 104 DOUBLE_TEAM
        true, // 105 RECOVER
        true, // 106 HARDEN
        true, // 107 MINIMIZE
        true, // 108 SMOKESCREEN
        true, // 109 CONFUSE_RAY
        true, // 110 WITHDRAW
        true, // 111 DEFENSE_CURL
        true, // 112 BARRIER
        true, // 113 LIGHT_SCREEN
        true, // 114 HAZE
        true, // 115 REFLECT
        true, // 116 FOCUS_ENERGY
        false, // 117 BIDE
        true, // 118 METRONOME
        true, // 119 MIRROR_MOVE
        true, // 120 SELF_DESTRUCT
        true, // 121 EGG_BOMB
        true, // 122 LICK
        false, // 123 SMOG
        false, // 124 SLUDGE
        false, // 125 BONE_CLUB
        true, // 126 FIRE_BLAST
        true, // 127 WATERFALL
        true, // 128 CLAMP
        true, // 129 SWIFT
        true, // 130 SKULL_BASH
        false, // 131 SPIKE_CANNON
        false, // 132 CONSTRICT
        true, // 133 AMNESIA
        true, // 134 KINESIS
        true, // 135 SOFT_BOILED
        true, // 136 HI_JUMP_KICK
        true, // 137 GLARE
        true, // 138 DREAM_EATER
        false, // 139 POISON_GAS
        false, // 140 BARRAGE
        false, // 141 LEECH_LIFE
        true, // 142 LOVELY_KISS
        true, // 143 SKY_ATTACK
        true, // 144 TRANSFORM
        false, // 145 BUBBLE
        true, // 146 DIZZY_PUNCH
        true, // 147 SPORE
        true, // 148 FLASH
        true, // 149 PSYWAVE
        false, // 150 SPLASH
        true, // 151 ACID_ARMOR
        true, // 152 CRABHAMMER
        true, // 153 EXPLOSION
        false, // 154 FURY_SWIPES
        true, // 155 BONEMERANG
        true, // 156 REST
        true, // 157 ROCK_SLIDE
        true, // 158 HYPER_FANG
        true, // 159 SHARPEN
        true, // 160 CONVERSION
        true, // 161 TRI_ATTACK
        true, // 162 SUPER_FANG
        true, // 163 SLASH
        true, // 164 SUBSTITUTE
        true, // 165 STRUGGLE
        true, // 166 SKETCH
        true, // 167 TRIPLE_KICK
        true, // 168 THIEF
        true, // 169 SPIDER_WEB
        true, // 170 MIND_READER
        true, // 171 NIGHTMARE
        false, // 172 FLAME_WHEEL
        true, // 173 SNORE
        true, // 174 CURSE
        true, // 175 FLAIL
        true, // 176 CONVERSION_2
        true, // 177 AEROBLAST
        true, // 178 COTTON_SPORE
        true, // 179 REVERSAL
        true, // 180 SPITE
        false, // 181 POWDER_SNOW
        true, // 182 PROTECT
        true, // 183 MACH_PUNCH
        true, // 184 SCARY_FACE
        true, // 185 FAINT_ATTACK
        true, // 186 SWEET_KISS
        true, // 187 BELLY_DRUM
        true, // 188 SLUDGE_BOMB
        true, // 189 MUD_SLAP
        true, // 190 OCTAZOOKA
        true, // 191 SPIKES
        true, // 192 ZAP_CANNON
        true, // 193 FORESIGHT
        true, // 194 DESTINY_BOND
        true, // 195 PERISH_SONG
        true, // 196 ICY_WIND
        true, // 197 DETECT
        false, // 198 BONE_RUSH
        true, // 199 LOCK_ON
        true, // 200 OUTRAGE
        true, // 201 SANDSTORM
        true, // 202 GIGA_DRAIN
        true, // 203 ENDURE
        true, // 204 CHARM
        true, // 205 ROLLOUT
        true, // 206 FALSE_SWIPE
        true, // 207 SWAGGER
        true, // 208 MILK_DRINK
        false, // 209 SPARK
        true, // 210 FURY_CUTTER
        true, // 211 STEEL_WING
        true, // 212 MEAN_LOOK
        true, // 213 ATTRACT
        true, // 214 SLEEP_TALK
        true, // 215 HEAL_BELL
        true, // 216 RETURN
        true, // 217 PRESENT
        true, // 218 FRUSTRATION
        true, // 219 SAFEGUARD
        true, // 220 PAIN_SPLIT
        true, // 221 SACRED_FIRE
        false, // 222 MAGNITUDE
        true, // 223 DYNAMIC_PUNCH
        true, // 224 MEGAHORN
        true, // 225 DRAGON_BREATH
        true, // 226 BATON_PASS
        true, // 227 ENCORE
        true, // 228 PURSUIT
        true, // 229 RAPID_SPIN
        true, // 230 SWEET_SCENT
        true, // 231 IRON_TAIL
        true, // 232 METAL_CLAW
        true, // 233 VITAL_THROW
        true, // 234 MORNING_SUN
        true, // 235 SYNTHESIS
        true, // 236 MOONLIGHT
        true, // 237 HIDDEN_POWER
        true, // 238 CROSS_CHOP
        false, // 239 TWISTER
        true, // 240 RAIN_DANCE
        true, // 241 SUNNY_DAY
        true, // 242 CRUNCH
        true, // 243 MIRROR_COAT
        true, // 244 PSYCH_UP
        true, // 245 EXTREME_SPEED
        true, // 246 ANCIENT_POWER
        true, // 247 SHADOW_BALL
        true, // 248 FUTURE_SIGHT
        true, // 249 ROCK_SMASH
        true, // 250 WHIRLPOOL
        true, // 251 BEAT_UP
        true, // 252 FAKE_OUT
        true, // 253 UPROAR
        true, // 254 STOCKPILE
        true, // 255 SPIT_UP
        true, // 256 SWALLOW
        true, // 257 HEAT_WAVE
        true, // 258 HAIL
        true, // 259 TORMENT
        true, // 260 FLATTER
        true, // 261 WILL_O_WISP
        true, // 262 MEMENTO
        true, // 263 FACADE
        true, // 264 FOCUS_PUNCH
        true, // 265 SMELLING_SALT
        true, // 266 FOLLOW_ME
        true, // 267 NATURE_POWER
        true, // 268 CHARGE
        true, // 269 TAUNT
        true, // 270 HELPING_HAND
        true, // 271 TRICK
        true, // 272 ROLE_PLAY
        true, // 273 WISH
        true, // 274 ASSIST
        true, // 275 INGRAIN
        true, // 276 SUPERPOWER
        true, // 277 MAGIC_COAT
        true, // 278 RECYCLE
        true, // 279 REVENGE
        true, // 280 BRICK_BREAK
        true, // 281 YAWN
        true, // 282 KNOCK_OFF
        true, // 283 ENDEAVOR
        true, // 284 ERUPTION
        true, // 285 SKILL_SWAP
        true, // 286 IMPRISON
        true, // 287 REFRESH
        true, // 288 GRUDGE
        true, // 289 SNATCH
        true, // 290 SECRET_POWER
        true, // 291 DIVE
        false, // 292 ARM_THRUST
        true, // 293 CAMOUFLAGE
        true, // 294 TAIL_GLOW
        true, // 295 LUSTER_PURGE
        true, // 296 MIST_BALL
        true, // 297 FEATHER_DANCE
        true, // 298 TEETER_DANCE
        true, // 299 BLAZE_KICK
        true, // 300 MUD_SPORT
        false, // 301 ICE_BALL
        true, // 302 NEEDLE_ARM
        true, // 303 SLACK_OFF
        true, // 304 HYPER_VOICE
        false, // 305 POISON_FANG
        true, // 306 CRUSH_CLAW
        true, // 307 BLAST_BURN
        true, // 308 HYDRO_CANNON
        true, // 309 METEOR_MASH
        true, // 310 ASTONISH
        true, // 311 WEATHER_BALL
        true, // 312 AROMATHERAPY
        true, // 313 FAKE_TEARS
        true, // 314 AIR_CUTTER
        true, // 315 OVERHEAT
        true, // 316 ODOR_SLEUTH
        true, // 317 ROCK_TOMB
        true, // 318 SILVER_WIND
        true, // 319 METAL_SOUND
        true, // 320 GRASS_WHISTLE
        true, // 321 TICKLE
        true, // 322 COSMIC_POWER
        true, // 323 WATER_SPOUT
        true, // 324 SIGNAL_BEAM
        true, // 325 SHADOW_PUNCH
        true, // 326 EXTRASENSORY
        true, // 327 SKY_UPPERCUT
        true, // 328 SAND_TOMB
        true, // 329 SHEER_COLD
        true, // 330 MUDDY_WATER
        false, // 331 BULLET_SEED
        true, // 332 AERIAL_ACE
        false, // 333 ICICLE_SPEAR
        true, // 334 IRON_DEFENSE
        true, // 335 BLOCK
        true, // 336 HOWL
        true, // 337 DRAGON_CLAW
        true, // 338 FRENZY_PLANT
        true, // 339 BULK_UP
        true, // 340 BOUNCE
        false, // 341 MUD_SHOT
        true, // 342 POISON_TAIL
        true, // 343 COVET
        true, // 344 VOLT_TACKLE
        true, // 345 MAGICAL_LEAF
        true, // 346 WATER_SPORT
        true, // 347 CALM_MIND
        true, // 348 LEAF_BLADE
        true, // 349 DRAGON_DANCE
        false, // 350 ROCK_BLAST
        true, // 351 SHOCK_WAVE
        true, // 352 WATER_PULSE
        true, // 353 DOOM_DESIRE
        true, // 354 PSYCHO_BOOST
    ];

    fn valid(index: u16) -> Result<bool, AssetError> {
        is_valid_apprentice_move(MoveId(index))
    }

    #[test]
    fn every_move_slot_matches_upstream() {
        for (index, expected) in EXPECTED.iter().enumerate() {
            let index = u16::try_from(index).expect("move index fits u16");
            assert_eq!(valid(index), Ok(*expected), "move {index}");
        }
    }

    #[test]
    fn valid_move_count_matches_the_table() {
        assert_eq!(APPRENTICE_VALID_MOVE_COUNT, 297);
        assert_eq!(EXPECTED.iter().filter(|&&valid| valid).count(), 297);
    }

    #[test]
    fn none_is_not_valid() {
        assert_eq!(is_valid_apprentice_move(MoveId::NONE), Ok(false));
    }

    #[test]
    fn double_slap_is_valid_but_comet_punch_is_not() {
        assert_eq!(is_valid_apprentice_move(MoveId::DOUBLE_SLAP), Ok(true));
        assert_eq!(is_valid_apprentice_move(MoveId::COMET_PUNCH), Ok(false));
    }

    #[test]
    fn aerial_ace_is_valid_but_icicle_spear_is_not() {
        assert_eq!(is_valid_apprentice_move(MoveId::AERIAL_ACE), Ok(true));
        assert_eq!(is_valid_apprentice_move(MoveId::ICICLE_SPEAR), Ok(false));
    }

    #[test]
    fn first_and_last_move_slots_are_covered() {
        assert_eq!(is_valid_apprentice_move(MoveId::POUND), Ok(false));
        assert_eq!(is_valid_apprentice_move(MoveId::PSYCHO_BOOST), Ok(true));
    }

    #[test]
    fn out_of_range_moves_are_errors_not_ineligible() {
        let first_invalid = u16::try_from(MOVES_COUNT).expect("move count fits u16");
        assert_eq!(
            valid(first_invalid),
            Err(AssetError::UnknownMove(first_invalid))
        );
        assert_eq!(valid(u16::MAX), Err(AssetError::UnknownMove(u16::MAX)));
    }
}
