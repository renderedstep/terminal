//! The dice, and the seed they are thrown from (`Roll` in the Ruby engine).
//!
//! A seed is five integers and plain arithmetic, nothing else, so the same
//! roll comes out the same in any process and any language.

use crate::random::Random;

pub const STORY: i128 = 1_000_003;
pub const PLAYTHROUGH: i128 = 100_003;
pub const AT: i128 = 10_007;
pub const SEQUENCE: i128 = 1_009;
pub const KIND: i128 = 100_000_007;

/// The kinds of roll; 0 is every roll that carves `sequence` up itself.
pub const THROW: i128 = 1;
pub const INTERIOR: i128 = 2;
pub const ITEM_POSITION: i128 = 3;
pub const CHARACTER_POSITION: i128 = 4;
pub const FOOTPRINT: i128 = 5;
pub const POPULATION: i128 = 6;
pub const VOLITION: i128 = 7;
pub const CAST: i128 = 8;

/// The five parts of a seed. Unset parts are 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seed {
    pub story: i128,
    pub playthrough: i128,
    pub at: i128,
    pub sequence: i128,
    pub kind: i128,
}

impl Seed {
    pub fn value(&self) -> i128 {
        self.story * STORY
            + self.playthrough * PLAYTHROUGH
            + self.at * AT
            + self.sequence * SEQUENCE
            + self.kind * KIND
    }

    /// `Roll.generator`: a fresh generator for one roll.
    pub fn generator(&self) -> Random {
        Random::new(self.value())
    }
}

/// One die: 1..=sides.
pub fn die(sides: i64, rng: &mut Random) -> i64 {
    rng.range(1, sides)
}

/// `count` dice added.
///
/// # Panics
/// When `count` is below one, as Ruby raises.
pub fn pool(count: i64, sides: i64, rng: &mut Random) -> i64 {
    assert!(count >= 1, "a pool is at least one die");
    (0..count).map(|_| die(sides, rng)).sum()
}

/// The index of one of `len` equally likely choices.
///
/// # Panics
/// When there is nothing to choose from.
pub fn one_of(len: usize, rng: &mut Random) -> usize {
    assert!(len > 0, "nothing to choose from");
    rng.below(len as u64) as usize
}

/// The index of one of `len` choices weighted by `weights`, from one draw.
/// Weights past `len` are ignored, missing ones count as 0, negatives are
/// clamped to 0, and a total of 0 falls back to an even draw.
pub fn weighted_one_of(len: usize, weights: &[i64], rng: &mut Random) -> usize {
    assert!(len > 0, "nothing to choose from");
    let weights: Vec<i64> = weights.iter().take(len).map(|w| (*w).max(0)).collect();
    let total: i64 = weights.iter().sum();
    if total <= 0 {
        return one_of(len, rng);
    }
    let mut target = rng.range(1, total);
    for index in 0..len {
        target -= weights.get(index).copied().unwrap_or(0);
        if target <= 0 {
            return index;
        }
    }
    len - 1
}
