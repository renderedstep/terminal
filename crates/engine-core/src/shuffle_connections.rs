//! Which way the mobile rooms' doorways turn on a shuffle: the arrangement
//! choice of `WorldMechanic::ShuffleConnections`, over rooms and doorways
//! given as values. Applying it (writing rows) stays with the caller.

use crate::random::Random;
use std::collections::{BTreeMap, BTreeSet};

pub const ATTEMPTS: usize = 32;
pub const SEED_STORY_MULTIPLIER: i128 = 1_000_003;

/// A doorway row: from `location_id` to `connected_location_id`.
pub type Edge = (i64, i64);

/// A story's rooms and doorways. `locations` is `(id, mobile)`; `connections`
/// are in the order they were written.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub story_id: i64,
    pub locations: Vec<(i64, bool)>,
    pub connections: Vec<Edge>,
}

impl Graph {
    /// `#anchor_edges`: every doorway from a mobile room to an anchored one,
    /// in written order.
    pub fn anchor_edges(&self) -> Vec<Edge> {
        let mobile = |id: i64| self.locations.iter().any(|&(l, m)| l == id && m);
        let anchored = |id: i64| self.locations.iter().any(|&(l, m)| l == id && !m);
        self.connections
            .iter()
            .copied()
            .filter(|&(from, to)| mobile(from) && anchored(to))
            .collect()
    }

    /// `#choose_arrangement`: the room each anchor edge leads to after the
    /// shuffle at `at` (epoch seconds), or `None` when no attempt gives a
    /// different, whole, connected world.
    pub fn choose_arrangement(&self, edges: &[Edge], at: i64) -> Option<Vec<i64>> {
        let current: Vec<i64> = edges.iter().map(|e| e.1).collect();
        let mut random =
            Random::new(i128::from(self.story_id) * SEED_STORY_MULTIPLIER + i128::from(at));
        let current_pairs = induced_pairs(edges, &current);
        for _ in 0..ATTEMPTS {
            let mut shuffled = current.clone();
            random.shuffle(&mut shuffled);
            let candidate = settle(edges, &current, &shuffled);
            if self.valid(edges, &candidate, &current_pairs) {
                return Some(candidate);
            }
        }
        None
    }

    fn valid(
        &self,
        edges: &[Edge],
        arrangement: &[i64],
        current_pairs: &BTreeSet<(i64, i64)>,
    ) -> bool {
        let pairs = induced_pairs(edges, arrangement);
        pairs.len() == edges.len() && &pairs != current_pairs && self.connected(edges, arrangement)
    }

    fn connected(&self, edges: &[Edge], arrangement: &[i64]) -> bool {
        let mut ids: Vec<i64> = self.locations.iter().map(|l| l.0).collect();
        ids.sort_unstable();
        if ids.len() <= 1 {
            return true;
        }
        let affected: BTreeSet<Edge> = edges.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect();
        let mut adjacency: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for &pair in &self.connections {
            if ids.binary_search(&pair.0).is_ok() && !affected.contains(&pair) {
                adjacency.entry(pair.0).or_default().push(pair.1);
            }
        }
        for (edge, &to) in edges.iter().zip(arrangement) {
            adjacency.entry(edge.0).or_default().push(to);
            adjacency.entry(to).or_default().push(edge.0);
        }
        let mut reached = BTreeSet::from([ids[0]]);
        let mut frontier = vec![ids[0]];
        while let Some(id) = frontier.pop() {
            for &neighbour in adjacency.get(&id).map_or(&[][..], Vec::as_slice) {
                if reached.insert(neighbour) {
                    frontier.push(neighbour);
                }
            }
        }
        reached.len() == ids.len()
    }
}

/// `#settle`: within each mobile room, a doorway that the shuffle could keep
/// where it was stays put, and the rest take what arrived in order.
fn settle(edges: &[Edge], current: &[i64], candidate: &[i64]) -> Vec<i64> {
    let mut settled = candidate.to_vec();
    // Group edge indices by room, rooms in order of first appearance.
    let mut groups: Vec<(i64, Vec<usize>)> = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        match groups.iter_mut().find(|(room, _)| *room == edge.0) {
            Some((_, indices)) => indices.push(index),
            None => groups.push((edge.0, vec![index])),
        }
    }
    for (_, indices) in groups {
        let mut arriving: Vec<i64> = indices.iter().map(|&i| candidate[i]).collect();
        let mut kept: BTreeMap<usize, i64> = BTreeMap::new();
        for &index in &indices {
            if let Some(found) = arriving.iter().position(|&a| a == current[index]) {
                arriving.remove(found);
                kept.insert(index, current[index]);
            }
        }
        let mut arriving = arriving.into_iter();
        for &index in &indices {
            settled[index] = match kept.get(&index) {
                Some(&k) => k,
                None => arriving.next().expect("one arrival per doorway"),
            };
        }
    }
    settled
}

fn induced_pairs(edges: &[Edge], arrangement: &[i64]) -> BTreeSet<(i64, i64)> {
    edges
        .iter()
        .zip(arrangement)
        .map(|(edge, &to)| (edge.0.min(to), edge.0.max(to)))
        .collect()
}
