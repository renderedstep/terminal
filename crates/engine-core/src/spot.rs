//! Where in a room one thing stands (`Location::Spot` and `Location::Placement`).

use crate::boxes::{Box, Spot};
use crate::random::Random;
use crate::roll::{self, Seed};

/// `Spot.inside`: a cell of the box, x drawn before y.
pub fn inside(room: &Box, rng: &mut Random) -> Spot {
    let x = room.x + roll::one_of(room.width as usize, rng) as i64;
    let y = room.y + roll::one_of(room.depth as usize, rng) as i64;
    Spot { x, y }
}

/// Which table a placed record comes from; each has its own kind of roll,
/// because their ids collide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    Item(i64),
    Character(i64),
}

/// The playthrough a placement belongs to, and its story time in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game {
    pub playthrough_id: i64,
    pub story_now: i64,
}

/// `Placement.in_the_world` (no game) or `Placement.in_a_game`. A room with
/// no box places nothing.
pub fn place(
    story_id: i64,
    room: Option<&Box>,
    record: Record,
    game: Option<Game>,
) -> Option<Spot> {
    let room = room?;
    let (kind, id) = match record {
        Record::Item(id) => (roll::ITEM_POSITION, id),
        Record::Character(id) => (roll::CHARACTER_POSITION, id),
    };
    let game = game.unwrap_or(Game {
        playthrough_id: 0,
        story_now: 0,
    });
    let mut rng = Seed {
        story: story_id.into(),
        playthrough: game.playthrough_id.into(),
        at: game.story_now.into(),
        sequence: id.into(),
        kind,
    }
    .generator();
    Some(inside(room, &mut rng))
}
