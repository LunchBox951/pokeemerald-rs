//! Generated address tables, one module per supported build.
//!
//! Nothing here is hand-written. `cargo xtask gen-rom-profile --rom <path>`
//! locates every root in a ROM whose whole-file SHA-1 already matched the
//! build it is generating for, asserts each root is unique, checks the
//! bytes each would yield against the pack `cargo xtask extract` produces,
//! and rewrites the module.
//!
//! Editing one by hand is how a wrong address gets shipped: the addresses
//! are only trustworthy because a machine derived and checked them. Change
//! the generator instead, and regenerate.

pub mod bpee_rev0;
