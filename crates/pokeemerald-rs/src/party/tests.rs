//! Party codec and merge tests, split by concern.

// The retained-record bodies spell `super::encode_attacks`.
use super::encode_attacks;

mod active_selection;
mod codec;
mod common;
mod create_mon;
mod loaded_lead;
mod merge_hp;
mod merge_stats;
mod retained_record;
mod stats;
