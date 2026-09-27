//! Where the engine puts what an overdue quest step needs: the anchor search
//! of `Quest::Deadline`, over a story's rooms and doorways given as values.

use std::collections::{BTreeMap, VecDeque};

pub const GRACE_ROOMS: i64 = 3;
/// `Location::ExitsSchema::MAX_EXITS`: a room with this many doorways out
/// takes no more.
pub const MAX_EXITS: usize = 4;

/// One room of the story, as the search reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: i64,
    pub realized: bool,
    /// The storey, or `None` for a room with no position (read as 0).
    pub z: Option<i64>,
    /// A place with a footprint whose rooms are already laid out.
    pub laid_out: bool,
}

/// `Quest::Deadline#hops`: a breadth-first walk from the lowest-id realized
/// room over doorways taken both ways, neighbours in id order. Empty when no
/// room is realized.
pub fn hops(rooms: &[Room], connections: &[(i64, i64)]) -> BTreeMap<i64, i64> {
    let Some(root) = rooms.iter().filter(|r| r.realized).map(|r| r.id).min() else {
        return BTreeMap::new();
    };
    let mut adjacency: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for &(from, to) in connections {
        adjacency.entry(from).or_default().push(to);
        adjacency.entry(to).or_default().push(from);
    }
    for neighbours in adjacency.values_mut() {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    let mut seen = BTreeMap::from([(root, 0)]);
    let mut queue = VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        let here = seen[&id];
        for &neighbour in adjacency.get(&id).map_or(&[][..], Vec::as_slice) {
            seen.entry(neighbour).or_insert_with(|| {
                queue.push_back(neighbour);
                here + 1
            });
        }
    }
    seen
}

/// `Quest::Deadline#anchor`: among reached rooms that are not laid out and
/// have fewer than `MAX_EXITS` doorways out, the lowest storey, then the
/// farthest, then the lowest id.
pub fn anchor(rooms: &[Room], connections: &[(i64, i64)]) -> Option<i64> {
    let hops = hops(rooms, connections);
    let exits = |id: i64| connections.iter().filter(|(from, _)| *from == id).count();
    rooms
        .iter()
        .filter(|room| hops.contains_key(&room.id))
        .filter(|room| !room.laid_out && exits(room.id) < MAX_EXITS)
        .min_by_key(|room| (room.z.unwrap_or(0), -hops[&room.id], room.id))
        .map(|room| room.id)
}
