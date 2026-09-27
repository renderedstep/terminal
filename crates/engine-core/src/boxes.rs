//! A room's rectangle on one storey, in paces (`Location::Box`).

/// A wall or doorway runs along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// A wall at a fixed x, running north to south.
    X,
    /// A wall at a fixed y, running west to east.
    Y,
}

impl Axis {
    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
        }
    }
}

/// The stretch of wall two boxes share: its axis, the line it sits on, and
/// where along that line it starts and stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedWall {
    pub axis: Axis,
    pub line: i64,
    pub from: i64,
    pub to: i64,
}

pub const METRES_PER_PACE: f64 = 1.5;
pub const MINIMUM_DOORWAY: i64 = 1;
pub const NORTH: &str = "north";
pub const EAST: &str = "east";
pub const SOUTH: &str = "south";
pub const WEST: &str = "west";
pub const WALLS: [&str; 4] = [NORTH, EAST, SOUTH, WEST];
pub const BEARING_SHARE: i64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Box {
    pub x: i64,
    pub y: i64,
    pub z: i64,
    pub width: i64,
    pub depth: i64,
}

/// A point inside a room, in paces (`Location::Spot`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub x: i64,
    pub y: i64,
}

impl Box {
    pub fn overlaps(&self, other: &Box) -> bool {
        self.z == other.z && self.shares_ground(other)
    }

    pub fn shares_ground(&self, other: &Box) -> bool {
        self.shared_ground(other).is_some()
    }

    pub fn shares_a_wall(&self, other: &Box) -> bool {
        self.shared_wall(other).is_some()
    }

    /// The wall the two boxes share on one storey, at least a doorway wide.
    pub fn shared_wall(&self, other: &Box) -> Option<SharedWall> {
        if self.z != other.z {
            return None;
        }
        let (axis, line, (from, to)) = if touching(self.x, self.width, other.x, other.width) {
            let line = if self.x + self.width == other.x {
                self.x + self.width
            } else {
                other.x + other.width
            };
            (
                Axis::X,
                line,
                span(self.y, self.depth, other.y, other.depth),
            )
        } else if touching(self.y, self.depth, other.y, other.depth) {
            let line = if self.y + self.depth == other.y {
                self.y + self.depth
            } else {
                other.y + other.depth
            };
            (
                Axis::Y,
                line,
                span(self.x, self.width, other.x, other.width),
            )
        } else {
            return None;
        };
        (to - from >= MINIMUM_DOORWAY).then_some(SharedWall {
            axis,
            line,
            from,
            to,
        })
    }

    /// Which of this box's walls the shared wall is.
    pub fn wall_towards(&self, other: &Box) -> Option<&'static str> {
        let wall = self.shared_wall(other)?;
        Some(match wall.axis {
            Axis::X if self.x + self.width == other.x => EAST,
            Axis::X => WEST,
            Axis::Y if self.y + self.depth == other.y => SOUTH,
            Axis::Y => NORTH,
        })
    }

    /// The rectangle both boxes cover, on this box's storey.
    pub fn shared_ground(&self, other: &Box) -> Option<Box> {
        let (left, right) = span(self.x, self.width, other.x, other.width);
        let (top, bottom) = span(self.y, self.depth, other.y, other.depth);
        (right > left && bottom > top).then_some(Box {
            x: left,
            y: top,
            z: self.z,
            width: right - left,
            depth: bottom - top,
        })
    }

    /// Where `part` lies from this box's centre: "north", "south-east", or
    /// `None` when it is near enough the middle on both axes.
    pub fn bearing_of(&self, part: &Box) -> Option<String> {
        let words: Vec<&str> = [
            offset_word(
                2 * part.y + part.depth - (2 * self.y + self.depth),
                self.depth,
                NORTH,
                SOUTH,
            ),
            offset_word(
                2 * part.x + part.width - (2 * self.x + self.width),
                self.width,
                WEST,
                EAST,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!words.is_empty()).then(|| words.join("-"))
    }

    pub fn metres(&self) -> [i64; 2] {
        [in_metres(self.width), in_metres(self.depth)]
    }

    pub fn inside_footprint(&self, footprint_width: i64, footprint_depth: i64) -> bool {
        self.x >= 0
            && self.y >= 0
            && self.x + self.width <= footprint_width
            && self.y + self.depth <= footprint_depth
    }

    pub fn contains(&self, spot: Spot) -> bool {
        spot.x >= self.x
            && spot.x < self.x + self.width
            && spot.y >= self.y
            && spot.y < self.y + self.depth
    }

    /// Centre to centre, walking along the axes.
    pub fn paces_to(&self, other: &Box) -> i64 {
        (((2 * self.x + self.width) - (2 * other.x + other.width)).abs()
            + ((2 * self.y + self.depth) - (2 * other.y + other.depth)).abs())
        .div_euclid(2)
    }
}

impl std::fmt::Display for Box {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{} paces at {},{} on storey {}",
            self.width, self.depth, self.x, self.y, self.z
        )
    }
}

fn touching(start: i64, extent: i64, other_start: i64, other_extent: i64) -> bool {
    start + extent == other_start || other_start + other_extent == start
}

fn span(start: i64, extent: i64, other_start: i64, other_extent: i64) -> (i64, i64) {
    (
        start.max(other_start),
        (start + extent).min(other_start + other_extent),
    )
}

fn offset_word(
    doubled: i64,
    extent: i64,
    before: &'static str,
    after: &'static str,
) -> Option<&'static str> {
    if BEARING_SHARE * doubled.abs() < 2 * extent {
        None
    } else if doubled < 0 {
        Some(before)
    } else {
        Some(after)
    }
}

/// Ruby's `Float#round`: half away from zero, which `f64::round` also does.
fn in_metres(paces: i64) -> i64 {
    (paces as f64 * METRES_PER_PACE).round() as i64
}
