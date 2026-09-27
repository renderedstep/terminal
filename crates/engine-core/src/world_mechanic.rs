//! When a world mechanic next runs (`WorldMechanic`'s boundary arithmetic).
//! Times are whole seconds since the Unix epoch.

/// A cadence: its period and its offset into the period, in minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cadence {
    pub period: i64,
    pub at: i64,
}

pub const CADENCES: [(&str, Cadence); 3] = [
    ("hourly", Cadence { period: 60, at: 0 }),
    (
        "nightly",
        Cadence {
            period: 1440,
            at: 0,
        },
    ),
    (
        "weekly",
        Cadence {
            period: 10_080,
            at: 0,
        },
    ),
];

pub fn cadence(name: &str) -> Option<Cadence> {
    CADENCES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

impl Cadence {
    /// The first boundary strictly after `at`. Ruby's integer division
    /// floors, so a time before the epoch still lands on a boundary.
    pub fn next_boundary_after(&self, at: i64) -> i64 {
        let period = self.period * 60;
        let offset = self.at * 60;
        ((at - offset).div_euclid(period) + 1) * period + offset
    }

    /// Every boundary after the last run (or the story's start, when it has
    /// never run) up to and including `now`.
    pub fn pending_boundaries(&self, from: i64, now: i64) -> Vec<i64> {
        if now <= from {
            return Vec::new();
        }
        let mut boundaries = Vec::new();
        let mut at = self.next_boundary_after(from);
        while at <= now {
            boundaries.push(at);
            at += self.period * 60;
        }
        boundaries
    }
}
