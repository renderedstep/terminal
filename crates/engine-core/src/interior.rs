//! How a building's inside is divided into rooms and doorways: the layout
//! decision of `Location::Interior`, returned as values instead of written.
//!
//! Every draw comes from one generator, in the Ruby order: the footprint (when
//! the place has none), the storey count, the basements, every storey's boxes,
//! each room's conditions as it is created, the stairs, then the extra doors.

use crate::boxes::Box;
use crate::danger::{self, Hazard};
use crate::parameters::Parameters;
use crate::random::Random;
use crate::roll::{self, Seed};

pub const MINIMUM_SIDE: i64 = 3;
pub const STOREYS: (i64, i64) = (1, 3);
pub const ROOMS_PER_STOREY: (i64, i64) = (2, 6);
pub const BASEMENTS: (i64, i64) = (0, 0);
pub const FOOTPRINT_SIDES: (i64, i64) = (9, 18);
pub const STAIRWELLS: (i64, i64) = (1, 2);
pub const DOOR_DIE: i64 = 6;
pub const EXTRA_DOOR_SHARE: i64 = 2;
pub const PACES_PER_MINUTE: i64 = 60;
/// `Location::ExitsSchema::MAX_EXITS`; the entry keeps one doorway spare
/// for the way in.
pub const MAX_EXITS: usize = 4;
pub const WALKING: &str = "walking";
pub const STAIRS: &str = "taking stairs";
/// `LocationConnection::DISTANCES`: a name and the most minutes it covers.
pub const DISTANCES: [(&str, i64); 5] = [
    ("adjacent", 1),
    ("a short walk", 5),
    ("across the district", 20),
    ("a long journey", 120),
    ("days away", 2880),
];

/// The place being laid out and the story around it.
#[derive(Clone, Debug)]
pub struct Place<'a> {
    pub story_id: i64,
    pub id: i64,
    pub name: &'a str,
    /// The story's clock in seconds, which seeds each new room's danger.
    pub clock: i64,
    /// How many locations the story holds before the first room is made,
    /// the place itself included.
    pub existing_locations: i64,
    /// `[width, depth]`, or `None` to roll one.
    pub footprint: Option<(i64, i64)>,
}

/// One room as it would be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub name: String,
    pub bounds: Box,
    pub danger: &'static str,
    pub hazard: Option<Hazard>,
}

/// One doorway row, by room index, in the order the rows would be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub distance: &'static str,
    pub travel_method: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub footprint: (i64, i64),
    /// In creation order; index 0 is the entry.
    pub rooms: Vec<Room>,
    pub edges: Vec<Edge>,
}

/// What a room is called until it is realized.
pub fn placeholder_name(place_name: &str, number: usize) -> String {
    format!("{place_name} room {number}")
}

/// `Interior.lay_out!`, without the writing. `below` is a count of storeys
/// under ground and wins over the parameters; `parameters` are the picks a
/// building was given, if any.
///
/// # Panics
/// When `below` is negative, which Ruby refuses too.
pub fn lay_out(place: &Place, below: Option<i64>, parameters: Option<&Parameters>) -> Layout {
    assert!(below.is_none_or(|b| b >= 0), "below is a count of storeys");
    let mut builder = Builder {
        rng: Seed {
            story: place.story_id.into(),
            sequence: place.id.into(),
            kind: roll::INTERIOR,
            ..Seed::default()
        }
        .generator(),
        rooms: Vec::new(),
        edges: Vec::new(),
    };
    let rng = &mut builder.rng;
    let (width, depth) = place.footprint.unwrap_or_else(|| {
        let width = from_range(FOOTPRINT_SIDES, rng);
        (width, from_range(FOOTPRINT_SIDES, rng))
    });
    let above = match parameters {
        Some(p) => p.above(),
        None => from_range(STOREYS, rng),
    };
    let basements = match (below, parameters) {
        (Some(b), _) => b,
        (None, Some(p)) => p.below(),
        (None, None) if BASEMENTS.0 == BASEMENTS.1 => BASEMENTS.0,
        (None, None) => from_range(BASEMENTS, rng),
    };
    let plans: Vec<Vec<Box>> = (0..above)
        .chain(-basements..0)
        .map(|z| builder.storey_boxes(width, depth, z))
        .collect();

    let mut storeys: Vec<Vec<usize>> = Vec::new();
    for boxes in plans {
        let storey = boxes
            .into_iter()
            .map(|bounds| builder.create_room(place, bounds, parameters))
            .collect();
        storeys.push(storey);
    }
    for storey in &storeys {
        for pair in storey.windows(2) {
            builder.connect(pair[0], pair[1], WALKING);
        }
    }
    let mut by_height = storeys.clone();
    by_height.sort_by_key(|storey| builder.rooms[storey[0]].bounds.z);
    for pair in by_height.windows(2) {
        builder.raise_stairs(&pair[0], &pair[1]);
    }
    for storey in &storeys {
        builder.open_extra_doors(storey);
    }
    builder.close_connectivity();
    Layout {
        footprint: (width, depth),
        rooms: builder.rooms,
        edges: builder.edges,
    }
}

/// `Roll.one_of(range.to_a)`.
fn from_range((low, high): (i64, i64), rng: &mut Random) -> i64 {
    low + roll::one_of((high - low + 1) as usize, rng) as i64
}

struct Builder {
    rng: Random,
    rooms: Vec<Room>,
    edges: Vec<Edge>,
}

impl Builder {
    fn storey_boxes(&mut self, width: i64, depth: i64, z: i64) -> Vec<Box> {
        let most_columns = (width / MINIMUM_SIDE).min(ROOMS_PER_STOREY.1).max(1);
        let columns = from_range((1, most_columns), &mut self.rng);
        let most_rows = (depth / MINIMUM_SIDE)
            .min(ROOMS_PER_STOREY.1 / columns)
            .max(1);
        let fewest_rows = ((ROOMS_PER_STOREY.0 + columns - 1) / columns)
            .max(1)
            .min(most_rows);
        let rows = from_range((fewest_rows, most_rows), &mut self.rng);
        let widths = self.share(width, columns);
        let depths = self.share(depth, rows);
        let lefts = offsets(&widths);
        let tops = offsets(&depths);
        self.serpentine(columns as usize, rows as usize)
            .into_iter()
            .map(|(column, row)| Box {
                x: lefts[column],
                y: tops[row],
                z,
                width: widths[column],
                depth: depths[row],
            })
            .collect()
    }

    /// Splits `length` into `parts`, each at least the minimum side where the
    /// length allows, the remainder handed out a pace at a time.
    fn share(&mut self, length: i64, parts: i64) -> Vec<i64> {
        let smallest = MINIMUM_SIDE.min(length.div_euclid(parts));
        let mut sizes = vec![smallest; parts as usize];
        for _ in 0..(length - smallest * parts) {
            sizes[roll::one_of(parts as usize, &mut self.rng)] += 1;
        }
        sizes
    }

    /// Cells in a snake, row by row or column by column, so neighbours in
    /// the list share a wall.
    fn serpentine(&mut self, columns: usize, rows: usize) -> Vec<(usize, usize)> {
        let by_rows = roll::one_of(2, &mut self.rng) == 0;
        let ordered = |count: usize, line: usize| -> Vec<usize> {
            if line.is_multiple_of(2) {
                (0..count).collect()
            } else {
                (0..count).rev().collect()
            }
        };
        if by_rows {
            (0..rows)
                .flat_map(|row| ordered(columns, row).into_iter().map(move |c| (c, row)))
                .collect()
        } else {
            (0..columns)
                .flat_map(|column| ordered(rows, column).into_iter().map(move |r| (column, r)))
                .collect()
        }
    }

    fn create_room(
        &mut self,
        place: &Place,
        bounds: Box,
        parameters: Option<&Parameters>,
    ) -> usize {
        let number = self.rooms.len() + 1;
        let mut room_danger = danger::for_a_new_room(
            place.story_id,
            place.clock,
            place.existing_locations + self.rooms.len() as i64,
        );
        let mut hazard = None;
        if let Some(p) = parameters {
            room_danger = danger::in_a_place(p, bounds.z, &mut self.rng);
            hazard = danger::hazard_in_a_place(p, bounds.z, &mut self.rng);
        }
        self.rooms.push(Room {
            name: placeholder_name(place.name, number),
            bounds,
            danger: room_danger,
            hazard,
        });
        number - 1
    }

    fn raise_stairs(&mut self, below: &[usize], above: &[usize]) {
        for _ in 0..from_range(STAIRWELLS, &mut self.rng) {
            let candidates: Vec<(usize, usize)> = below
                .iter()
                .flat_map(|&one| above.iter().map(move |&other| (one, other)))
                .filter(|&(one, other)| {
                    self.rooms[one]
                        .bounds
                        .shares_ground(&self.rooms[other].bounds)
                        && !self.connected(one, other)
                        && self.room_to_spare(one)
                        && self.room_to_spare(other)
                })
                .collect();
            if candidates.is_empty() {
                break;
            }
            let (one, other) = candidates[roll::one_of(candidates.len(), &mut self.rng)];
            self.connect(one, other, STAIRS);
        }
    }

    fn open_extra_doors(&mut self, storey: &[usize]) {
        for (i, &one) in storey.iter().enumerate() {
            for &other in &storey[i + 1..] {
                if !self.rooms[one]
                    .bounds
                    .shares_a_wall(&self.rooms[other].bounds)
                {
                    continue;
                }
                if roll::die(DOOR_DIE, &mut self.rng) <= EXTRA_DOOR_SHARE {
                    self.connect(one, other, WALKING);
                }
            }
        }
    }

    /// Joins any room the doorways so far leave stranded, nearest first.
    fn close_connectivity(&mut self) {
        if self.rooms.is_empty() {
            return;
        }
        loop {
            let reached = self.reachable_from(0);
            let stranded: Vec<usize> = (0..self.rooms.len())
                .filter(|r| !reached.contains(r))
                .collect();
            if stranded.is_empty() {
                break;
            }
            let Some((one, other)) = self.joinable(&reached, &stranded) else {
                break;
            };
            let method = if self.rooms[one].bounds.z == self.rooms[other].bounds.z {
                WALKING
            } else {
                STAIRS
            };
            if !self.connect(one, other, method) {
                break;
            }
        }
    }

    fn reachable_from(&self, entry: usize) -> Vec<usize> {
        let mut seen = vec![entry];
        let mut next = 0;
        while next < seen.len() {
            let room = seen[next];
            next += 1;
            for edge in self.edges.iter().filter(|e| e.from == room) {
                if !seen.contains(&edge.to) {
                    seen.push(edge.to);
                }
            }
        }
        seen
    }

    fn joinable(&self, reached: &[usize], stranded: &[usize]) -> Option<(usize, usize)> {
        reached
            .iter()
            .flat_map(|&one| stranded.iter().map(move |&other| (one, other)))
            .find(|&(one, other)| {
                let (a, b) = (&self.rooms[one].bounds, &self.rooms[other].bounds);
                !self.connected(one, other)
                    && self.room_to_spare(one)
                    && self.room_to_spare(other)
                    && (a.shares_a_wall(b) || ((a.z - b.z).abs() == 1 && a.shares_ground(b)))
            })
    }

    fn connect(&mut self, one: usize, other: usize, travel_method: &'static str) -> bool {
        if self.connected(one, other) || !self.room_to_spare(one) || !self.room_to_spare(other) {
            return false;
        }
        let distance = distance_for(self.rooms[one].bounds.paces_to(&self.rooms[other].bounds));
        for (from, to) in [(one, other), (other, one)] {
            self.edges.push(Edge {
                from,
                to,
                distance,
                travel_method,
            });
        }
        true
    }

    fn connected(&self, one: usize, other: usize) -> bool {
        self.edges
            .iter()
            .any(|e| (e.from == one && e.to == other) || (e.from == other && e.to == one))
    }

    fn room_to_spare(&self, room: usize) -> bool {
        let cap = if room == 0 { MAX_EXITS - 1 } else { MAX_EXITS };
        self.edges.iter().filter(|e| e.from == room).count() < cap
    }
}

fn offsets(sizes: &[i64]) -> Vec<i64> {
    let mut all = vec![0];
    for size in sizes {
        all.push(all.last().copied().unwrap_or(0) + size);
    }
    all
}

/// The first distance whose walk covers the minutes, a pace a second.
pub fn distance_for(paces: i64) -> &'static str {
    let minutes = ((paces + PACES_PER_MINUTE - 1).div_euclid(PACES_PER_MINUTE)).max(1);
    DISTANCES
        .iter()
        .find(|(_, walk)| *walk >= minutes)
        .map_or(DISTANCES[DISTANCES.len() - 1].0, |(name, _)| name)
}
