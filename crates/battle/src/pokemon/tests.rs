//! Splits the battle Pokémon test suite by independent seam: stat
//! calculation, battler lifecycle, PP-Up bonuses, and EV storage with
//! EV-aware level-up.

mod shared;

mod battler_lifecycle;
mod ev;
mod pp;
mod stat_math;
