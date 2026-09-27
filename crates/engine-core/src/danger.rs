//! How dangerous a room is, and whether its people are monsters: the pure
//! half of `Location::Danger`.

use crate::parameters::{Parameters, HAZARD_DIE};
use crate::random::Random;
use crate::roll::{self, Seed};

pub const SEQUENCE_BASE: i64 = 1_000_000;
pub const ROLLED: [&str; 8] = [
    "safe",
    "safe",
    "safe",
    "safe",
    "safe",
    "uneasy",
    "uneasy",
    "dangerous",
];
pub const DANGERS: [(&str, i64); 4] = [("safe", 0), ("uneasy", 1), ("dangerous", 3), ("deadly", 6)];
pub const DANGER_DIE: i64 = 6;
pub const HAZARD_DICE: [i64; 4] = [4, 6, 8, 10];

/// `Danger.for_a_new_room`: a story's clock in seconds and how many rooms
/// it already holds.
pub fn for_a_new_room(story_id: i64, clock: i64, rooms: i64) -> &'static str {
    let mut rng = Seed {
        story: story_id.into(),
        at: clock.into(),
        sequence: (SEQUENCE_BASE + rooms).into(),
        ..Seed::default()
    }
    .generator();
    ROLLED[roll::one_of(ROLLED.len(), &mut rng)]
}

/// `Danger.generator_for`: the generator a room's cast is drawn from.
pub fn generator_for(story_id: i64, location_id: i64) -> Random {
    Seed {
        story: story_id.into(),
        sequence: (SEQUENCE_BASE + location_id).into(),
        ..Seed::default()
    }
    .generator()
}

/// `Location#danger_share`: faces of the danger die that make a monster.
pub fn danger_share(danger: &str) -> i64 {
    DANGERS
        .iter()
        .find(|(name, _)| *name == danger)
        .map_or(0, |(_, share)| *share)
}

/// `Danger.monstrous?`: no throw at all in a safe room.
pub fn monstrous(danger: &str, rng: &mut Random) -> bool {
    let share = danger_share(danger);
    share > 0 && roll::die(DANGER_DIE, rng) <= share
}

/// `Danger.in_a_place`: one face of the storey's danger table.
pub fn in_a_place(parameters: &Parameters, storey: i64, rng: &mut Random) -> &'static str {
    let faces = parameters.danger_for(storey);
    faces[roll::one_of(faces.len(), rng)]
}

/// A room's hazard: what it is and the die its save is thrown against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hazard {
    pub hazard: &'static str,
    pub hazard_die: i64,
}

/// `Danger.hazard_in_a_place`.
pub fn hazard_in_a_place(parameters: &Parameters, storey: i64, rng: &mut Random) -> Option<Hazard> {
    let share = parameters.hazard_share_for(storey);
    if share <= 0 || roll::die(HAZARD_DIE, rng) > share {
        return None;
    }
    Some(Hazard {
        hazard: parameters.hazard(),
        hazard_die: HAZARD_DICE[roll::one_of(HAZARD_DICE.len(), rng)],
    })
}
