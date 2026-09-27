//! How many people a room holds (`Location::Population`).
//!
//! Alone among the rolls, this one is seeded from the room's name rather than
//! its row, because a re-loaded world is the same place with new row ids.

use crate::random::Random;
use crate::roll::{self, Seed};
use crate::text::{crc32, natural_key};

pub const BANDS: [(&str, &[i64]); 3] = [
    ("nobody", &[0]),
    ("a person or two", &[1, 1, 2]),
    ("a crowd", &[2, 3, 3]),
];
pub const ROLLED: [&str; 4] = ["nobody", "nobody", "a person or two", "a crowd"];
pub const LEADING_ARTICLE: &str = r"\A(?:the|a|an)\s+";

/// `Population.key_for`: the CRC32 of the name's natural key.
pub fn key_for(name: &str) -> u32 {
    crc32(natural_key(name).as_bytes())
}

pub fn generator_for(name: &str) -> Random {
    Seed {
        story: 0,
        sequence: key_for(name).into(),
        kind: roll::POPULATION,
        ..Seed::default()
    }
    .generator()
}

fn band(label: &str) -> Option<&'static [i64]> {
    BANDS
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, counts)| *counts)
}

/// `Population.label_for`: the stored word if it is one, otherwise rolled.
pub fn label_for(stored: Option<&str>, rng: &mut Random) -> &'static str {
    if let Some((name, _)) = stored.and_then(|s| BANDS.iter().find(|(name, _)| *name == s)) {
        return name;
    }
    ROLLED[roll::one_of(ROLLED.len(), rng)]
}

/// `Population.draw`: a head count from a label's band.
///
/// # Panics
/// When `label` is not one of the bands.
pub fn draw(label: &str, rng: &mut Random) -> i64 {
    let counts = band(label).expect("a population label");
    counts[roll::one_of(counts.len(), rng)]
}

/// The label and head count for a room, both from one generator.
pub fn for_room(name: &str, stored: Option<&str>) -> (&'static str, i64) {
    let mut rng = generator_for(name);
    let label = label_for(stored, &mut rng);
    (label, draw(label, &mut rng))
}
