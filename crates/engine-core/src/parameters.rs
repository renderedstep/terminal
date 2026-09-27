//! A building's shape as a model picked it from closed tables
//! (`Location::Parameters`). Every table keeps the Ruby key order, because
//! the order is what the danger ladder and the fallbacks read.

use crate::random::Random;
use crate::roll;

pub const NO_INSIDE: &str = "no inside";
pub const ONE_ROOM: &str = "one room";
pub const INSIDE: [(&str, Option<(i64, i64)>); 4] = [
    (NO_INSIDE, None),
    (ONE_ROOM, None),
    ("a few rooms", Some((6, 9))),
    ("a warren of rooms", Some((12, 18))),
];
pub const ROOMS_A_BAND_PROMISES: [(&str, i64); 3] =
    [(ONE_ROOM, 1), ("a few rooms", 2), ("a warren of rooms", 4)];
pub const STOREYS_ABOVE: [(&str, i64); 3] = [
    ("ground floor only", 1),
    ("one storey up", 2),
    ("two storeys up", 3),
];
pub const STOREYS_BELOW: [(&str, i64); 4] = [
    ("none", 0),
    ("a cellar", 1),
    ("two levels down", 2),
    ("deep", 3),
];
pub const SAFE: &str = "safe";
pub const DANGER: [(&str, [&str; 8]); 3] = [
    (
        "safe",
        [
            "safe", "safe", "safe", "safe", "safe", "safe", "safe", "uneasy",
        ],
    ),
    (
        "uneasy",
        [
            "safe",
            "safe",
            "safe",
            "safe",
            "uneasy",
            "uneasy",
            "uneasy",
            "dangerous",
        ],
    ),
    (
        "dangerous",
        [
            "safe",
            "safe",
            "uneasy",
            "uneasy",
            "uneasy",
            "dangerous",
            "dangerous",
            "dangerous",
        ],
    ),
];
pub const LADDER: [&str; 3] = ["safe", "uneasy", "dangerous"];
pub const FLAT: &str = "the same throughout";
pub const GRADIENT: [(&str, i64); 3] = [
    (FLAT, 0),
    ("worse the deeper you go", -1),
    ("worse the higher you climb", 1),
];
pub const NO_HAZARD: &str = "none";
pub const HAZARDS: [&str; 5] = [NO_HAZARD, "flooded", "unlit", "silent", "airless"];
pub const HAZARD_DIE: i64 = 6;
pub const HAZARD_SHARE: i64 = 2;

/// The picks as written. A missing or unknown value falls back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parameters {
    pub inside: Option<String>,
    pub storeys_above: Option<String>,
    pub storeys_below: Option<String>,
    pub danger: Option<String>,
    pub gradient: Option<String>,
    pub hazard: Option<String>,
}

fn pick<'a, V>(table: &'a [(&'a str, V)], picked: &Option<String>, fallback: &'a str) -> &'a str {
    picked
        .as_deref()
        .and_then(|p| table.iter().find(|(key, _)| *key == p))
        .map_or(fallback, |(key, _)| key)
}

fn fetch<V: Copy>(table: &[(&str, V)], key: &str) -> V {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .expect("a key the table holds")
}

impl Parameters {
    pub fn inside(&self) -> &'static str {
        pick(&INSIDE, &self.inside, NO_INSIDE)
    }

    pub fn has_inside(&self) -> bool {
        self.inside() != NO_INSIDE
    }

    /// `[width, depth]`, each drawn from the band, or `None` with no band.
    pub fn footprint(&self, rng: &mut Random) -> Option<(i64, i64)> {
        let (low, high) = fetch(&INSIDE, self.inside())?;
        let len = (high - low + 1) as usize;
        let width = low + roll::one_of(len, rng) as i64;
        let depth = low + roll::one_of(len, rng) as i64;
        Some((width, depth))
    }

    pub fn storeys_above(&self) -> &'static str {
        pick(&STOREYS_ABOVE, &self.storeys_above, STOREYS_ABOVE[0].0)
    }

    pub fn storeys_below(&self) -> &'static str {
        pick(&STOREYS_BELOW, &self.storeys_below, STOREYS_BELOW[0].0)
    }

    pub fn above(&self) -> i64 {
        fetch(&STOREYS_ABOVE, self.storeys_above())
    }

    pub fn below(&self) -> i64 {
        fetch(&STOREYS_BELOW, self.storeys_below())
    }

    pub fn danger(&self) -> &'static str {
        pick(&DANGER, &self.danger, SAFE)
    }

    pub fn gradient(&self) -> &'static str {
        pick(&GRADIENT, &self.gradient, FLAT)
    }

    pub fn hazard(&self) -> &'static str {
        self.hazard
            .as_deref()
            .and_then(|h| HAZARDS.iter().find(|k| **k == h))
            .copied()
            .unwrap_or(NO_HAZARD)
    }

    pub fn has_hazard(&self) -> bool {
        self.hazard() != NO_HAZARD
    }

    /// The eight-faced danger table for one storey.
    pub fn danger_for(&self, storey: i64) -> [&'static str; 8] {
        fetch(&DANGER, LADDER[self.rung_for(storey)])
    }

    pub fn hazard_share_for(&self, storey: i64) -> i64 {
        if !self.has_hazard() {
            return 0;
        }
        (HAZARD_SHARE + self.step_for(storey)).clamp(0, HAZARD_DIE)
    }

    fn step_for(&self, storey: i64) -> i64 {
        (fetch(&GRADIENT, self.gradient()) * storey).clamp(-1, 1)
    }

    fn rung_for(&self, storey: i64) -> usize {
        let rung = LADDER.iter().position(|r| *r == self.danger()).unwrap_or(0) as i64;
        (rung + self.step_for(storey)).clamp(0, LADDER.len() as i64 - 1) as usize
    }
}
