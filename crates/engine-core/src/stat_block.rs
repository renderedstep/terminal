//! A body's numbers, rolled rather than written (`Character::StatBlock`).

use crate::roll::{self, Seed};

pub const HIT_DICE: [i64; 3] = [6, 8, 10];
pub const ABILITIES: [&str; 3] = ["strength", "dexterity", "will"];
pub const STARTING_LEVEL: i64 = 1;
pub const PROTAGONIST_LEVEL: i64 = 3;
pub const PROTAGONIST_HIT_DIE: i64 = 8;
pub const ABILITY_DICE: i64 = 3;
pub const ABILITY_SIDES: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatBlock {
    pub level: i64,
    pub hit_die: i64,
    pub strength: i64,
    pub dexterity: i64,
    pub will: i64,
}

/// `StatBlock.roll`: the hit die, then 3d6 for each ability in order.
pub fn roll(story: i64, at: i64, sequence: i64) -> StatBlock {
    let mut rng = Seed {
        story: story.into(),
        at: at.into(),
        sequence: sequence.into(),
        ..Seed::default()
    }
    .generator();
    let hit_die = HIT_DICE[roll::one_of(HIT_DICE.len(), &mut rng)];
    let mut ability = || roll::pool(ABILITY_DICE, ABILITY_SIDES, &mut rng);
    StatBlock {
        level: STARTING_LEVEL,
        hit_die,
        strength: ability(),
        dexterity: ability(),
        will: ability(),
    }
}

/// `StatBlock.for_a_protagonist`: the same roll, at a higher level and a
/// fixed hit die.
pub fn for_a_protagonist(story: i64, at: i64, sequence: i64) -> StatBlock {
    StatBlock {
        level: PROTAGONIST_LEVEL,
        hit_die: PROTAGONIST_HIT_DIE,
        ..roll(story, at, sequence)
    }
}
