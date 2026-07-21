//! Per-move display names (S-4): the `gMoveNames` table.
//!
//! Ports the flat display-name strings from the upstream reference
//! `pokeemerald/src/data/text/move_names.h`
//! (`gMoveNames[MOVES_COUNT][MOVE_NAME_LENGTH + 1]`), keyed by [`MoveId`].
//! Slot `0` is `MOVE_NONE` (`"-"` upstream); real moves run `1..MOVES_COUNT`
//! in the same `MOVE_*` id order as
//! [`MoveTable`](crate::battle_moves::MoveTable).
//!
//! Names are transcribed verbatim as upstream renders them on the GBA
//! charmap `(behavioral-fidelity)` — including the hyphenated
//! `SAND-ATTACK`/`DOUBLE-EDGE` and the concatenated `DOUBLESLAP`/`SONICBOOM`/
//! `THUNDERPUNCH`/`VICEGRIP`, none of which upstream spaces out.
//!
//! Re-expressed as an owned `&'static [&'static str]` table rather than the C
//! designated-initializer array `(no-verbatim, oop-boundaries)`; the
//! upstream-tie tests below pin a sample of names straight from
//! `move_names.h` so the transcription cannot silently drift
//! `(behavioral-fidelity)`.

use crate::battle_moves::MoveId;
use crate::error::AssetError;
use crate::MOVES_COUNT;

/// The extracted `gMoveNames` table, indexed by [`MoveId`] `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct MoveNames {
    names: &'static [&'static str],
}

impl MoveNames {
    /// The number of addressable [`MoveId`] slots, including `MOVE_NONE`.
    /// Matches upstream `MOVES_COUNT`.
    pub const LEN: usize = NAMES.len();

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { names: &NAMES }
    }

    /// The display name for `mv`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMove`] if `mv` is outside the extracted
    /// range `0..MOVES_COUNT`.
    pub fn name(&self, mv: MoveId) -> Result<&'static str, AssetError> {
        self.names
            .get(mv.0 as usize)
            .copied()
            .ok_or(AssetError::UnknownMove(mv.0))
    }

    /// The number of addressable move-name slots (including `MOVE_NONE`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the table has no entries (never true for the extracted data).
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

/// The transcribed `gMoveNames` table, indexed by `MOVE_*` id. `0` is the
/// reserved `MOVE_NONE` slot.
const NAMES: [&str; MOVES_COUNT] = [
    "-",            // 0 MOVE_NONE
    "POUND",        // 1 MOVE_POUND
    "KARATE CHOP",  // 2 MOVE_KARATE_CHOP
    "DOUBLESLAP",   // 3 MOVE_DOUBLE_SLAP
    "COMET PUNCH",  // 4 MOVE_COMET_PUNCH
    "MEGA PUNCH",   // 5 MOVE_MEGA_PUNCH
    "PAY DAY",      // 6 MOVE_PAY_DAY
    "FIRE PUNCH",   // 7 MOVE_FIRE_PUNCH
    "ICE PUNCH",    // 8 MOVE_ICE_PUNCH
    "THUNDERPUNCH", // 9 MOVE_THUNDER_PUNCH
    "SCRATCH",      // 10 MOVE_SCRATCH
    "VICEGRIP",     // 11 MOVE_VICE_GRIP
    "GUILLOTINE",   // 12 MOVE_GUILLOTINE
    "RAZOR WIND",   // 13 MOVE_RAZOR_WIND
    "SWORDS DANCE", // 14 MOVE_SWORDS_DANCE
    "CUT",          // 15 MOVE_CUT
    "GUST",         // 16 MOVE_GUST
    "WING ATTACK",  // 17 MOVE_WING_ATTACK
    "WHIRLWIND",    // 18 MOVE_WHIRLWIND
    "FLY",          // 19 MOVE_FLY
    "BIND",         // 20 MOVE_BIND
    "SLAM",         // 21 MOVE_SLAM
    "VINE WHIP",    // 22 MOVE_VINE_WHIP
    "STOMP",        // 23 MOVE_STOMP
    "DOUBLE KICK",  // 24 MOVE_DOUBLE_KICK
    "MEGA KICK",    // 25 MOVE_MEGA_KICK
    "JUMP KICK",    // 26 MOVE_JUMP_KICK
    "ROLLING KICK", // 27 MOVE_ROLLING_KICK
    "SAND-ATTACK",  // 28 MOVE_SAND_ATTACK
    "HEADBUTT",     // 29 MOVE_HEADBUTT
    "HORN ATTACK",  // 30 MOVE_HORN_ATTACK
    "FURY ATTACK",  // 31 MOVE_FURY_ATTACK
    "HORN DRILL",   // 32 MOVE_HORN_DRILL
    "TACKLE",       // 33 MOVE_TACKLE
    "BODY SLAM",    // 34 MOVE_BODY_SLAM
    "WRAP",         // 35 MOVE_WRAP
    "TAKE DOWN",    // 36 MOVE_TAKE_DOWN
    "THRASH",       // 37 MOVE_THRASH
    "DOUBLE-EDGE",  // 38 MOVE_DOUBLE_EDGE
    "TAIL WHIP",    // 39 MOVE_TAIL_WHIP
    "POISON STING", // 40 MOVE_POISON_STING
    "TWINEEDLE",    // 41 MOVE_TWINEEDLE
    "PIN MISSILE",  // 42 MOVE_PIN_MISSILE
    "LEER",         // 43 MOVE_LEER
    "BITE",         // 44 MOVE_BITE
    "GROWL",        // 45 MOVE_GROWL
    "ROAR",         // 46 MOVE_ROAR
    "SING",         // 47 MOVE_SING
    "SUPERSONIC",   // 48 MOVE_SUPERSONIC
    "SONICBOOM",    // 49 MOVE_SONIC_BOOM
    "DISABLE",      // 50 MOVE_DISABLE
    "ACID",         // 51 MOVE_ACID
    "EMBER",        // 52 MOVE_EMBER
    "FLAMETHROWER", // 53 MOVE_FLAMETHROWER
    "MIST",         // 54 MOVE_MIST
    "WATER GUN",    // 55 MOVE_WATER_GUN
    "HYDRO PUMP",   // 56 MOVE_HYDRO_PUMP
    "SURF",         // 57 MOVE_SURF
    "ICE BEAM",     // 58 MOVE_ICE_BEAM
    "BLIZZARD",     // 59 MOVE_BLIZZARD
    "PSYBEAM",      // 60 MOVE_PSYBEAM
    "BUBBLEBEAM",   // 61 MOVE_BUBBLE_BEAM
    "AURORA BEAM",  // 62 MOVE_AURORA_BEAM
    "HYPER BEAM",   // 63 MOVE_HYPER_BEAM
    "PECK",         // 64 MOVE_PECK
    "DRILL PECK",   // 65 MOVE_DRILL_PECK
    "SUBMISSION",   // 66 MOVE_SUBMISSION
    "LOW KICK",     // 67 MOVE_LOW_KICK
    "COUNTER",      // 68 MOVE_COUNTER
    "SEISMIC TOSS", // 69 MOVE_SEISMIC_TOSS
    "STRENGTH",     // 70 MOVE_STRENGTH
    "ABSORB",       // 71 MOVE_ABSORB
    "MEGA DRAIN",   // 72 MOVE_MEGA_DRAIN
    "LEECH SEED",   // 73 MOVE_LEECH_SEED
    "GROWTH",       // 74 MOVE_GROWTH
    "RAZOR LEAF",   // 75 MOVE_RAZOR_LEAF
    "SOLARBEAM",    // 76 MOVE_SOLAR_BEAM
    "POISONPOWDER", // 77 MOVE_POISON_POWDER
    "STUN SPORE",   // 78 MOVE_STUN_SPORE
    "SLEEP POWDER", // 79 MOVE_SLEEP_POWDER
    "PETAL DANCE",  // 80 MOVE_PETAL_DANCE
    "STRING SHOT",  // 81 MOVE_STRING_SHOT
    "DRAGON RAGE",  // 82 MOVE_DRAGON_RAGE
    "FIRE SPIN",    // 83 MOVE_FIRE_SPIN
    "THUNDERSHOCK", // 84 MOVE_THUNDER_SHOCK
    "THUNDERBOLT",  // 85 MOVE_THUNDERBOLT
    "THUNDER WAVE", // 86 MOVE_THUNDER_WAVE
    "THUNDER",      // 87 MOVE_THUNDER
    "ROCK THROW",   // 88 MOVE_ROCK_THROW
    "EARTHQUAKE",   // 89 MOVE_EARTHQUAKE
    "FISSURE",      // 90 MOVE_FISSURE
    "DIG",          // 91 MOVE_DIG
    "TOXIC",        // 92 MOVE_TOXIC
    "CONFUSION",    // 93 MOVE_CONFUSION
    "PSYCHIC",      // 94 MOVE_PSYCHIC
    "HYPNOSIS",     // 95 MOVE_HYPNOSIS
    "MEDITATE",     // 96 MOVE_MEDITATE
    "AGILITY",      // 97 MOVE_AGILITY
    "QUICK ATTACK", // 98 MOVE_QUICK_ATTACK
    "RAGE",         // 99 MOVE_RAGE
    "TELEPORT",     // 100 MOVE_TELEPORT
    "NIGHT SHADE",  // 101 MOVE_NIGHT_SHADE
    "MIMIC",        // 102 MOVE_MIMIC
    "SCREECH",      // 103 MOVE_SCREECH
    "DOUBLE TEAM",  // 104 MOVE_DOUBLE_TEAM
    "RECOVER",      // 105 MOVE_RECOVER
    "HARDEN",       // 106 MOVE_HARDEN
    "MINIMIZE",     // 107 MOVE_MINIMIZE
    "SMOKESCREEN",  // 108 MOVE_SMOKESCREEN
    "CONFUSE RAY",  // 109 MOVE_CONFUSE_RAY
    "WITHDRAW",     // 110 MOVE_WITHDRAW
    "DEFENSE CURL", // 111 MOVE_DEFENSE_CURL
    "BARRIER",      // 112 MOVE_BARRIER
    "LIGHT SCREEN", // 113 MOVE_LIGHT_SCREEN
    "HAZE",         // 114 MOVE_HAZE
    "REFLECT",      // 115 MOVE_REFLECT
    "FOCUS ENERGY", // 116 MOVE_FOCUS_ENERGY
    "BIDE",         // 117 MOVE_BIDE
    "METRONOME",    // 118 MOVE_METRONOME
    "MIRROR MOVE",  // 119 MOVE_MIRROR_MOVE
    "SELFDESTRUCT", // 120 MOVE_SELF_DESTRUCT
    "EGG BOMB",     // 121 MOVE_EGG_BOMB
    "LICK",         // 122 MOVE_LICK
    "SMOG",         // 123 MOVE_SMOG
    "SLUDGE",       // 124 MOVE_SLUDGE
    "BONE CLUB",    // 125 MOVE_BONE_CLUB
    "FIRE BLAST",   // 126 MOVE_FIRE_BLAST
    "WATERFALL",    // 127 MOVE_WATERFALL
    "CLAMP",        // 128 MOVE_CLAMP
    "SWIFT",        // 129 MOVE_SWIFT
    "SKULL BASH",   // 130 MOVE_SKULL_BASH
    "SPIKE CANNON", // 131 MOVE_SPIKE_CANNON
    "CONSTRICT",    // 132 MOVE_CONSTRICT
    "AMNESIA",      // 133 MOVE_AMNESIA
    "KINESIS",      // 134 MOVE_KINESIS
    "SOFTBOILED",   // 135 MOVE_SOFT_BOILED
    "HI JUMP KICK", // 136 MOVE_HI_JUMP_KICK
    "GLARE",        // 137 MOVE_GLARE
    "DREAM EATER",  // 138 MOVE_DREAM_EATER
    "POISON GAS",   // 139 MOVE_POISON_GAS
    "BARRAGE",      // 140 MOVE_BARRAGE
    "LEECH LIFE",   // 141 MOVE_LEECH_LIFE
    "LOVELY KISS",  // 142 MOVE_LOVELY_KISS
    "SKY ATTACK",   // 143 MOVE_SKY_ATTACK
    "TRANSFORM",    // 144 MOVE_TRANSFORM
    "BUBBLE",       // 145 MOVE_BUBBLE
    "DIZZY PUNCH",  // 146 MOVE_DIZZY_PUNCH
    "SPORE",        // 147 MOVE_SPORE
    "FLASH",        // 148 MOVE_FLASH
    "PSYWAVE",      // 149 MOVE_PSYWAVE
    "SPLASH",       // 150 MOVE_SPLASH
    "ACID ARMOR",   // 151 MOVE_ACID_ARMOR
    "CRABHAMMER",   // 152 MOVE_CRABHAMMER
    "EXPLOSION",    // 153 MOVE_EXPLOSION
    "FURY SWIPES",  // 154 MOVE_FURY_SWIPES
    "BONEMERANG",   // 155 MOVE_BONEMERANG
    "REST",         // 156 MOVE_REST
    "ROCK SLIDE",   // 157 MOVE_ROCK_SLIDE
    "HYPER FANG",   // 158 MOVE_HYPER_FANG
    "SHARPEN",      // 159 MOVE_SHARPEN
    "CONVERSION",   // 160 MOVE_CONVERSION
    "TRI ATTACK",   // 161 MOVE_TRI_ATTACK
    "SUPER FANG",   // 162 MOVE_SUPER_FANG
    "SLASH",        // 163 MOVE_SLASH
    "SUBSTITUTE",   // 164 MOVE_SUBSTITUTE
    "STRUGGLE",     // 165 MOVE_STRUGGLE
    "SKETCH",       // 166 MOVE_SKETCH
    "TRIPLE KICK",  // 167 MOVE_TRIPLE_KICK
    "THIEF",        // 168 MOVE_THIEF
    "SPIDER WEB",   // 169 MOVE_SPIDER_WEB
    "MIND READER",  // 170 MOVE_MIND_READER
    "NIGHTMARE",    // 171 MOVE_NIGHTMARE
    "FLAME WHEEL",  // 172 MOVE_FLAME_WHEEL
    "SNORE",        // 173 MOVE_SNORE
    "CURSE",        // 174 MOVE_CURSE
    "FLAIL",        // 175 MOVE_FLAIL
    "CONVERSION 2", // 176 MOVE_CONVERSION_2
    "AEROBLAST",    // 177 MOVE_AEROBLAST
    "COTTON SPORE", // 178 MOVE_COTTON_SPORE
    "REVERSAL",     // 179 MOVE_REVERSAL
    "SPITE",        // 180 MOVE_SPITE
    "POWDER SNOW",  // 181 MOVE_POWDER_SNOW
    "PROTECT",      // 182 MOVE_PROTECT
    "MACH PUNCH",   // 183 MOVE_MACH_PUNCH
    "SCARY FACE",   // 184 MOVE_SCARY_FACE
    "FAINT ATTACK", // 185 MOVE_FAINT_ATTACK
    "SWEET KISS",   // 186 MOVE_SWEET_KISS
    "BELLY DRUM",   // 187 MOVE_BELLY_DRUM
    "SLUDGE BOMB",  // 188 MOVE_SLUDGE_BOMB
    "MUD-SLAP",     // 189 MOVE_MUD_SLAP
    "OCTAZOOKA",    // 190 MOVE_OCTAZOOKA
    "SPIKES",       // 191 MOVE_SPIKES
    "ZAP CANNON",   // 192 MOVE_ZAP_CANNON
    "FORESIGHT",    // 193 MOVE_FORESIGHT
    "DESTINY BOND", // 194 MOVE_DESTINY_BOND
    "PERISH SONG",  // 195 MOVE_PERISH_SONG
    "ICY WIND",     // 196 MOVE_ICY_WIND
    "DETECT",       // 197 MOVE_DETECT
    "BONE RUSH",    // 198 MOVE_BONE_RUSH
    "LOCK-ON",      // 199 MOVE_LOCK_ON
    "OUTRAGE",      // 200 MOVE_OUTRAGE
    "SANDSTORM",    // 201 MOVE_SANDSTORM
    "GIGA DRAIN",   // 202 MOVE_GIGA_DRAIN
    "ENDURE",       // 203 MOVE_ENDURE
    "CHARM",        // 204 MOVE_CHARM
    "ROLLOUT",      // 205 MOVE_ROLLOUT
    "FALSE SWIPE",  // 206 MOVE_FALSE_SWIPE
    "SWAGGER",      // 207 MOVE_SWAGGER
    "MILK DRINK",   // 208 MOVE_MILK_DRINK
    "SPARK",        // 209 MOVE_SPARK
    "FURY CUTTER",  // 210 MOVE_FURY_CUTTER
    "STEEL WING",   // 211 MOVE_STEEL_WING
    "MEAN LOOK",    // 212 MOVE_MEAN_LOOK
    "ATTRACT",      // 213 MOVE_ATTRACT
    "SLEEP TALK",   // 214 MOVE_SLEEP_TALK
    "HEAL BELL",    // 215 MOVE_HEAL_BELL
    "RETURN",       // 216 MOVE_RETURN
    "PRESENT",      // 217 MOVE_PRESENT
    "FRUSTRATION",  // 218 MOVE_FRUSTRATION
    "SAFEGUARD",    // 219 MOVE_SAFEGUARD
    "PAIN SPLIT",   // 220 MOVE_PAIN_SPLIT
    "SACRED FIRE",  // 221 MOVE_SACRED_FIRE
    "MAGNITUDE",    // 222 MOVE_MAGNITUDE
    "DYNAMICPUNCH", // 223 MOVE_DYNAMIC_PUNCH
    "MEGAHORN",     // 224 MOVE_MEGAHORN
    "DRAGONBREATH", // 225 MOVE_DRAGON_BREATH
    "BATON PASS",   // 226 MOVE_BATON_PASS
    "ENCORE",       // 227 MOVE_ENCORE
    "PURSUIT",      // 228 MOVE_PURSUIT
    "RAPID SPIN",   // 229 MOVE_RAPID_SPIN
    "SWEET SCENT",  // 230 MOVE_SWEET_SCENT
    "IRON TAIL",    // 231 MOVE_IRON_TAIL
    "METAL CLAW",   // 232 MOVE_METAL_CLAW
    "VITAL THROW",  // 233 MOVE_VITAL_THROW
    "MORNING SUN",  // 234 MOVE_MORNING_SUN
    "SYNTHESIS",    // 235 MOVE_SYNTHESIS
    "MOONLIGHT",    // 236 MOVE_MOONLIGHT
    "HIDDEN POWER", // 237 MOVE_HIDDEN_POWER
    "CROSS CHOP",   // 238 MOVE_CROSS_CHOP
    "TWISTER",      // 239 MOVE_TWISTER
    "RAIN DANCE",   // 240 MOVE_RAIN_DANCE
    "SUNNY DAY",    // 241 MOVE_SUNNY_DAY
    "CRUNCH",       // 242 MOVE_CRUNCH
    "MIRROR COAT",  // 243 MOVE_MIRROR_COAT
    "PSYCH UP",     // 244 MOVE_PSYCH_UP
    "EXTREMESPEED", // 245 MOVE_EXTREME_SPEED
    "ANCIENTPOWER", // 246 MOVE_ANCIENT_POWER
    "SHADOW BALL",  // 247 MOVE_SHADOW_BALL
    "FUTURE SIGHT", // 248 MOVE_FUTURE_SIGHT
    "ROCK SMASH",   // 249 MOVE_ROCK_SMASH
    "WHIRLPOOL",    // 250 MOVE_WHIRLPOOL
    "BEAT UP",      // 251 MOVE_BEAT_UP
    "FAKE OUT",     // 252 MOVE_FAKE_OUT
    "UPROAR",       // 253 MOVE_UPROAR
    "STOCKPILE",    // 254 MOVE_STOCKPILE
    "SPIT UP",      // 255 MOVE_SPIT_UP
    "SWALLOW",      // 256 MOVE_SWALLOW
    "HEAT WAVE",    // 257 MOVE_HEAT_WAVE
    "HAIL",         // 258 MOVE_HAIL
    "TORMENT",      // 259 MOVE_TORMENT
    "FLATTER",      // 260 MOVE_FLATTER
    "WILL-O-WISP",  // 261 MOVE_WILL_O_WISP
    "MEMENTO",      // 262 MOVE_MEMENTO
    "FACADE",       // 263 MOVE_FACADE
    "FOCUS PUNCH",  // 264 MOVE_FOCUS_PUNCH
    "SMELLINGSALT", // 265 MOVE_SMELLING_SALT
    "FOLLOW ME",    // 266 MOVE_FOLLOW_ME
    "NATURE POWER", // 267 MOVE_NATURE_POWER
    "CHARGE",       // 268 MOVE_CHARGE
    "TAUNT",        // 269 MOVE_TAUNT
    "HELPING HAND", // 270 MOVE_HELPING_HAND
    "TRICK",        // 271 MOVE_TRICK
    "ROLE PLAY",    // 272 MOVE_ROLE_PLAY
    "WISH",         // 273 MOVE_WISH
    "ASSIST",       // 274 MOVE_ASSIST
    "INGRAIN",      // 275 MOVE_INGRAIN
    "SUPERPOWER",   // 276 MOVE_SUPERPOWER
    "MAGIC COAT",   // 277 MOVE_MAGIC_COAT
    "RECYCLE",      // 278 MOVE_RECYCLE
    "REVENGE",      // 279 MOVE_REVENGE
    "BRICK BREAK",  // 280 MOVE_BRICK_BREAK
    "YAWN",         // 281 MOVE_YAWN
    "KNOCK OFF",    // 282 MOVE_KNOCK_OFF
    "ENDEAVOR",     // 283 MOVE_ENDEAVOR
    "ERUPTION",     // 284 MOVE_ERUPTION
    "SKILL SWAP",   // 285 MOVE_SKILL_SWAP
    "IMPRISON",     // 286 MOVE_IMPRISON
    "REFRESH",      // 287 MOVE_REFRESH
    "GRUDGE",       // 288 MOVE_GRUDGE
    "SNATCH",       // 289 MOVE_SNATCH
    "SECRET POWER", // 290 MOVE_SECRET_POWER
    "DIVE",         // 291 MOVE_DIVE
    "ARM THRUST",   // 292 MOVE_ARM_THRUST
    "CAMOUFLAGE",   // 293 MOVE_CAMOUFLAGE
    "TAIL GLOW",    // 294 MOVE_TAIL_GLOW
    "LUSTER PURGE", // 295 MOVE_LUSTER_PURGE
    "MIST BALL",    // 296 MOVE_MIST_BALL
    "FEATHERDANCE", // 297 MOVE_FEATHER_DANCE
    "TEETER DANCE", // 298 MOVE_TEETER_DANCE
    "BLAZE KICK",   // 299 MOVE_BLAZE_KICK
    "MUD SPORT",    // 300 MOVE_MUD_SPORT
    "ICE BALL",     // 301 MOVE_ICE_BALL
    "NEEDLE ARM",   // 302 MOVE_NEEDLE_ARM
    "SLACK OFF",    // 303 MOVE_SLACK_OFF
    "HYPER VOICE",  // 304 MOVE_HYPER_VOICE
    "POISON FANG",  // 305 MOVE_POISON_FANG
    "CRUSH CLAW",   // 306 MOVE_CRUSH_CLAW
    "BLAST BURN",   // 307 MOVE_BLAST_BURN
    "HYDRO CANNON", // 308 MOVE_HYDRO_CANNON
    "METEOR MASH",  // 309 MOVE_METEOR_MASH
    "ASTONISH",     // 310 MOVE_ASTONISH
    "WEATHER BALL", // 311 MOVE_WEATHER_BALL
    "AROMATHERAPY", // 312 MOVE_AROMATHERAPY
    "FAKE TEARS",   // 313 MOVE_FAKE_TEARS
    "AIR CUTTER",   // 314 MOVE_AIR_CUTTER
    "OVERHEAT",     // 315 MOVE_OVERHEAT
    "ODOR SLEUTH",  // 316 MOVE_ODOR_SLEUTH
    "ROCK TOMB",    // 317 MOVE_ROCK_TOMB
    "SILVER WIND",  // 318 MOVE_SILVER_WIND
    "METAL SOUND",  // 319 MOVE_METAL_SOUND
    "GRASSWHISTLE", // 320 MOVE_GRASS_WHISTLE
    "TICKLE",       // 321 MOVE_TICKLE
    "COSMIC POWER", // 322 MOVE_COSMIC_POWER
    "WATER SPOUT",  // 323 MOVE_WATER_SPOUT
    "SIGNAL BEAM",  // 324 MOVE_SIGNAL_BEAM
    "SHADOW PUNCH", // 325 MOVE_SHADOW_PUNCH
    "EXTRASENSORY", // 326 MOVE_EXTRASENSORY
    "SKY UPPERCUT", // 327 MOVE_SKY_UPPERCUT
    "SAND TOMB",    // 328 MOVE_SAND_TOMB
    "SHEER COLD",   // 329 MOVE_SHEER_COLD
    "MUDDY WATER",  // 330 MOVE_MUDDY_WATER
    "BULLET SEED",  // 331 MOVE_BULLET_SEED
    "AERIAL ACE",   // 332 MOVE_AERIAL_ACE
    "ICICLE SPEAR", // 333 MOVE_ICICLE_SPEAR
    "IRON DEFENSE", // 334 MOVE_IRON_DEFENSE
    "BLOCK",        // 335 MOVE_BLOCK
    "HOWL",         // 336 MOVE_HOWL
    "DRAGON CLAW",  // 337 MOVE_DRAGON_CLAW
    "FRENZY PLANT", // 338 MOVE_FRENZY_PLANT
    "BULK UP",      // 339 MOVE_BULK_UP
    "BOUNCE",       // 340 MOVE_BOUNCE
    "MUD SHOT",     // 341 MOVE_MUD_SHOT
    "POISON TAIL",  // 342 MOVE_POISON_TAIL
    "COVET",        // 343 MOVE_COVET
    "VOLT TACKLE",  // 344 MOVE_VOLT_TACKLE
    "MAGICAL LEAF", // 345 MOVE_MAGICAL_LEAF
    "WATER SPORT",  // 346 MOVE_WATER_SPORT
    "CALM MIND",    // 347 MOVE_CALM_MIND
    "LEAF BLADE",   // 348 MOVE_LEAF_BLADE
    "DRAGON DANCE", // 349 MOVE_DRAGON_DANCE
    "ROCK BLAST",   // 350 MOVE_ROCK_BLAST
    "SHOCK WAVE",   // 351 MOVE_SHOCK_WAVE
    "WATER PULSE",  // 352 MOVE_WATER_PULSE
    "DOOM DESIRE",  // 353 MOVE_DOOM_DESIRE
    "PSYCHO BOOST", // 354 MOVE_PSYCHO_BOOST
];

#[cfg(test)]
mod tests {
    use super::{MoveNames, NAMES};
    use crate::battle_moves::MoveId;
    use crate::error::AssetError;
    use crate::MOVES_COUNT;

    #[test]
    fn structural_length_matches_upstream() {
        assert_eq!(MoveNames::LEN, MOVES_COUNT);
        assert_eq!(MoveNames::LEN, 355);
        assert_eq!(NAMES.len(), MoveNames::LEN);
        let table = MoveNames::new();
        assert_eq!(table.len(), 355);
        assert!(!table.is_empty());
    }

    #[test]
    fn upstream_tie_sampled_names() {
        // Read straight from move_names.h.
        let table = MoveNames::new();
        assert_eq!(table.name(MoveId(0)), Ok("-")); // MOVE_NONE
        assert_eq!(table.name(MoveId(1)), Ok("POUND"));
        assert_eq!(table.name(MoveId(3)), Ok("DOUBLESLAP")); // MOVE_DOUBLE_SLAP
        assert_eq!(table.name(MoveId(28)), Ok("SAND-ATTACK"));
        assert_eq!(table.name(MoveId(38)), Ok("DOUBLE-EDGE"));
        assert_eq!(table.name(MoveId(49)), Ok("SONICBOOM"));
        assert_eq!(table.name(MoveId(89)), Ok("EARTHQUAKE"));
        assert_eq!(table.name(MoveId(94)), Ok("PSYCHIC"));
        assert_eq!(table.name(MoveId(351)), Ok("SHOCK WAVE"));
        assert_eq!(table.name(MoveId(352)), Ok("WATER PULSE"));
        assert_eq!(table.name(MoveId(353)), Ok("DOOM DESIRE"));
        assert_eq!(table.name(MoveId(354)), Ok("PSYCHO BOOST")); // last move
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
        // Every move upstream has a distinct display name; a duplicate would
        // mean a mis-transcribed id -> name mapping.
        for (i, a) in NAMES.iter().enumerate() {
            for b in &NAMES[i + 1..] {
                assert_ne!(a, b, "duplicate move name {a:?}");
            }
        }
    }
}
