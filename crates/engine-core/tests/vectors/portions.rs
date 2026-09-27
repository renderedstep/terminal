//! One test per portion: how to read its inputs, call this crate, and write
//! its answers in the shape the vectors record.

use crate::check;
use engine_core::boxes::{Box, Spot};
use engine_core::parameters::{self, Parameters};
use engine_core::random::Random;
use engine_core::{danger, population, roll, spot, stat_block};
use serde_json::{json, Value};

pub const NAMES: &[&str] = &[
    "roll",
    "stat_block",
    "spot",
    "placement",
    "population",
    "danger",
    "parameters",
    "box",
    "interior",
    "shuffle_connections",
    "world_mechanic",
    "deadline",
    "cast",
];

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("an integer, not {value}"))
}

fn opt_str(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn box_of(value: &Value) -> Box {
    Box {
        x: int(&value["x"]),
        y: int(&value["y"]),
        z: int(&value["z"]),
        width: int(&value["width"]),
        depth: int(&value["depth"]),
    }
}

fn box_json(b: &Box) -> Value {
    json!({ "x": b.x, "y": b.y, "z": b.z, "width": b.width, "depth": b.depth })
}

/// A table as `[key, value]` pairs, the way the vectors write one.
fn pairs<K: Into<Value> + Copy, V>(table: &[(K, V)], value: impl Fn(&V) -> Value) -> Value {
    Value::Array(
        table
            .iter()
            .map(|(k, v)| json!([(*k).into(), value(v)]))
            .collect(),
    )
}

fn picks_of(value: &Value) -> Parameters {
    Parameters {
        inside: opt_str(&value["inside"]),
        storeys_above: opt_str(&value["storeys_above"]),
        storeys_below: opt_str(&value["storeys_below"]),
        danger: opt_str(&value["danger"]),
        gradient: opt_str(&value["gradient"]),
        hazard: opt_str(&value["hazard"]),
    }
}

#[test]
fn roll() {
    let constants = json!({
        "multipliers": [["story", 1_000_003], ["playthrough", 100_003], ["at", 10_007],
                        ["sequence", 1_009], ["kind", 100_000_007]],
        "kinds": [["THROW", 1], ["INTERIOR", 2], ["ITEM_POSITION", 3], ["CHARACTER_POSITION", 4],
                  ["FOOTPRINT", 5], ["POPULATION", 6], ["VOLITION", 7], ["CAST", 8]],
    });
    assert_eq!(
        [
            roll::STORY,
            roll::PLAYTHROUGH,
            roll::AT,
            roll::SEQUENCE,
            roll::KIND
        ],
        [1_000_003, 100_003, 10_007, 1_009, 100_000_007]
    );
    assert_eq!(
        [
            roll::THROW,
            roll::INTERIOR,
            roll::ITEM_POSITION,
            roll::CHARACTER_POSITION,
            roll::FOOTPRINT,
            roll::POPULATION,
            roll::VOLITION,
            roll::CAST
        ],
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    check("roll", constants, |input| {
        if let Some(ops) = input.get("ops") {
            let seed: i128 = input["seed"]
                .as_str()
                .expect("a seed string")
                .parse()
                .expect("a seed");
            let mut rng = Random::new(seed);
            return ops
                .as_array()
                .expect("ops")
                .iter()
                .map(|op| draw(op, &mut rng))
                .collect();
        }
        let part = |key: &str| input.get(key).map_or(0, |v| i128::from(int(v)));
        let seed = roll::Seed {
            story: part("story"),
            playthrough: part("playthrough"),
            at: part("at"),
            sequence: part("sequence"),
            kind: part("kind"),
        };
        json!(seed.value().to_string())
    });
}

fn draw(op: &Value, rng: &mut Random) -> Value {
    let size = || int(&op["size"]) as usize;
    match op["op"].as_str().expect("an op") {
        "die" => json!(roll::die(int(&op["sides"]), rng)),
        "pool" => json!(roll::pool(int(&op["count"]), int(&op["sides"]), rng)),
        "one_of" => json!(roll::one_of(size(), rng)),
        "weighted" => {
            let weights: Vec<i64> = op["weights"]
                .as_array()
                .expect("weights")
                .iter()
                .map(int)
                .collect();
            json!(roll::weighted_one_of(weights.len(), &weights, rng))
        }
        "range" => json!(rng.range(int(&op["min"]), int(&op["max"]))),
        "shuffle" => {
            let mut items: Vec<usize> = (0..size()).collect();
            rng.shuffle(&mut items);
            json!(items)
        }
        other => panic!("unknown op {other}"),
    }
}

#[test]
fn stat_block() {
    let constants = json!({
        "hit_dice": stat_block::HIT_DICE,
        "abilities": stat_block::ABILITIES,
        "starting_level": stat_block::STARTING_LEVEL,
        "protagonist_level": stat_block::PROTAGONIST_LEVEL,
        "protagonist_hit_die": stat_block::PROTAGONIST_HIT_DIE,
        "ability_dice": stat_block::ABILITY_DICE,
        "ability_sides": stat_block::ABILITY_SIDES,
    });
    check("stat_block", constants, |input| {
        let (story, at, sequence) = (
            int(&input["story"]),
            int(&input["at"]),
            int(&input["sequence"]),
        );
        let block = if input["protagonist"] == true {
            stat_block::for_a_protagonist(story, at, sequence)
        } else {
            stat_block::roll(story, at, sequence)
        };
        json!({ "level": block.level, "hit_die": block.hit_die, "strength": block.strength,
                "dexterity": block.dexterity, "will": block.will })
    });
}

#[test]
fn spot() {
    check("spot", json!({}), |input| {
        let room = box_of(&input["box"]);
        let mut rng = Random::new(int(&input["seed"]).into());
        (0..int(&input["draws"]))
            .map(|_| {
                let at = spot::inside(&room, &mut rng);
                json!([at.x, at.y])
            })
            .collect()
    });
}

#[test]
fn placement() {
    let constants = json!({ "kinds": [["Item", roll::ITEM_POSITION as i64], ["Character", roll::CHARACTER_POSITION as i64]] });
    check("placement", constants, |input| {
        let room = &input["room"];
        let placed = room["x"].is_i64().then(|| box_of(room));
        let id = int(&input["record"]["id"]);
        let record = match input["record"]["kind"].as_str() {
            Some("Item") => spot::Record::Item(id),
            Some("Character") => spot::Record::Character(id),
            other => panic!("unknown record kind {other:?}"),
        };
        let game = input["game"].as_object().map(|game| spot::Game {
            playthrough_id: int(&game["playthrough_id"]),
            story_now: int(&game["story_now"]),
        });
        match spot::place(int(&room["story_id"]), placed.as_ref(), record, game) {
            Some(at) => json!({ "x": at.x, "y": at.y }),
            None => json!({ "x": null, "y": null }),
        }
    });
}

#[test]
fn population() {
    let constants = json!({
        "bands": pairs(&population::BANDS, |counts| json!(counts)),
        "rolled": population::ROLLED,
        "leading_article": population::LEADING_ARTICLE,
    });
    check("population", constants, |input| {
        let name = input["name"].as_str().expect("a name");
        let (label, count) = population::for_room(name, input["population"].as_str());
        json!({
            "natural_key": engine_core::text::natural_key(name),
            "key": population::key_for(name),
            "label": label,
            "count": count,
        })
    });
}

#[test]
fn danger() {
    let constants = json!({
        "sequence_base": danger::SEQUENCE_BASE,
        "rolled": danger::ROLLED,
        "dangers": pairs(&danger::DANGERS, |share| json!(share)),
        "danger_die": danger::DANGER_DIE,
        "hazard_dice": danger::HAZARD_DICE,
    });
    check("danger", constants, |input| match input["kind"].as_str() {
        Some("new_room") => json!(danger::for_a_new_room(
            int(&input["story_id"]),
            int(&input["clock"]),
            int(&input["rooms"])
        )),
        Some("monstrous") => {
            let mut rng =
                danger::generator_for(int(&input["story_id"]), int(&input["location_id"]));
            let room_danger = input["danger"].as_str().expect("a danger");
            (0..int(&input["throws"]))
                .map(|_| json!(danger::monstrous(room_danger, &mut rng)))
                .collect()
        }
        Some("in_a_place") => {
            let parameters = picks_of(&input["picks"]);
            let mut rng = Random::new(int(&input["seed"]).into());
            input["storeys"]
                .as_array()
                .expect("storeys")
                .iter()
                .map(|storey| {
                    let storey = int(storey);
                    let room_danger = danger::in_a_place(&parameters, storey, &mut rng);
                    let hazard = danger::hazard_in_a_place(&parameters, storey, &mut rng);
                    json!({
                        "danger": room_danger,
                        "hazard": hazard.map(|h| h.hazard),
                        "hazard_die": hazard.map(|h| h.hazard_die),
                    })
                })
                .collect()
        }
        other => panic!("unknown danger case {other:?}"),
    });
}

#[test]
fn parameters() {
    let constants = json!({
        "inside": pairs(&parameters::INSIDE, |band| match band {
            Some((low, high)) => json!([low, high]),
            None => Value::Null,
        }),
        "rooms_a_band_promises": pairs(&parameters::ROOMS_A_BAND_PROMISES, |n| json!(n)),
        "storeys_above": pairs(&parameters::STOREYS_ABOVE, |n| json!(n)),
        "storeys_below": pairs(&parameters::STOREYS_BELOW, |n| json!(n)),
        "danger": pairs(&parameters::DANGER, |faces| json!(faces)),
        "ladder": parameters::LADDER,
        "gradient": pairs(&parameters::GRADIENT, |n| json!(n)),
        "hazards": parameters::HAZARDS,
        "hazard_die": parameters::HAZARD_DIE,
        "hazard_share": parameters::HAZARD_SHARE,
    });
    check("parameters", constants, |input| {
        let p = picks_of(&input["picks"]);
        match input["kind"].as_str() {
            Some("resolve") => {
                let storeys = -3..=3;
                json!({
                    "inside": p.inside(), "inside?": p.has_inside(),
                    "storeys_above": p.storeys_above(), "storeys_below": p.storeys_below(),
                    "above": p.above(), "below": p.below(),
                    "danger": p.danger(), "gradient": p.gradient(),
                    "hazard": p.hazard(), "hazard?": p.has_hazard(),
                    "danger_for": storeys.clone().map(|s| json!([s, p.danger_for(s)])).collect::<Vec<_>>(),
                    "hazard_share_for": storeys.map(|s| json!([s, p.hazard_share_for(s)])).collect::<Vec<_>>(),
                })
            }
            Some("footprint") => {
                let mut rng = Random::new(int(&input["seed"]).into());
                match p.footprint(&mut rng) {
                    Some((width, depth)) => json!([width, depth]),
                    None => Value::Null,
                }
            }
            other => panic!("unknown parameters case {other:?}"),
        }
    });
}

#[test]
fn boxes() {
    use engine_core::boxes;
    let constants = json!({
        "metres_per_pace": boxes::METRES_PER_PACE,
        "minimum_doorway": boxes::MINIMUM_DOORWAY,
        "walls": boxes::WALLS,
        "bearing_share": boxes::BEARING_SHARE,
    });
    check("box", constants, |input| {
        let (a, b) = (box_of(&input["a"]), box_of(&input["b"]));
        let footprint = &input["footprint"];
        let at = Spot {
            x: int(&input["spot"][0]),
            y: int(&input["spot"][1]),
        };
        json!({
            "overlaps": a.overlaps(&b),
            "shares_ground": a.shares_ground(&b),
            "shares_a_wall": a.shares_a_wall(&b),
            "shared_wall": a.shared_wall(&b).map(|w| json!([w.axis.name(), w.line, w.from, w.to])),
            "wall_towards": a.wall_towards(&b),
            "shared_ground": a.shared_ground(&b).map(|g| box_json(&g)),
            "bearing_of": a.bearing_of(&b),
            "metres": a.metres(),
            "inside_footprint": a.inside_footprint(int(&footprint[0]), int(&footprint[1])),
            "contains": a.contains(at),
            "paces_to": a.paces_to(&b),
            "to_s": a.to_string(),
        })
    });
}

#[test]
fn world_mechanic() {
    use engine_core::world_mechanic::{cadence, CADENCES};
    let constants = json!({
        "cadences": pairs(&CADENCES, |c| json!([["period", c.period], ["at", c.at]])),
    });
    check("world_mechanic", constants, |input| {
        let cadence =
            cadence(input["cadence"].as_str().expect("a cadence")).expect("a known cadence");
        let from = input["last_run_at"]
            .as_i64()
            .unwrap_or_else(|| int(&input["start_time"]));
        json!({
            "next_boundary_after": cadence.next_boundary_after(from),
            "pending": cadence.pending_boundaries(from, int(&input["now"])),
        })
    });
}

fn id_pairs(value: &Value) -> Vec<(i64, i64)> {
    value
        .as_array()
        .expect("pairs")
        .iter()
        .map(|pair| (int(&pair[0]), int(&pair[1])))
        .collect()
}

#[test]
fn deadline() {
    use engine_core::deadline::{self, Room, GRACE_ROOMS, MAX_EXITS};
    let constants = json!({ "max_exits": MAX_EXITS, "grace_rooms": GRACE_ROOMS });
    check("deadline", constants, |input| {
        let rooms: Vec<Room> = input["rooms"]
            .as_array()
            .expect("rooms")
            .iter()
            .map(|room| Room {
                id: int(&room["id"]),
                realized: room["realized"] == true,
                z: room["z"].as_i64(),
                // A place is written with its one room inside, so it is laid out.
                laid_out: room["place"] == true,
            })
            .collect();
        let connections = id_pairs(&input["connections"]);
        let hops: Vec<Value> = deadline::hops(&rooms, &connections)
            .into_iter()
            .map(|(id, hops)| json!([id, hops]))
            .collect();
        json!({ "hops": hops, "anchor": deadline::anchor(&rooms, &connections) })
    });
}

#[test]
fn shuffle_connections() {
    use engine_core::shuffle_connections::{Graph, ATTEMPTS, SEED_STORY_MULTIPLIER};
    let constants =
        json!({ "attempts": ATTEMPTS, "seed_story_multiplier": SEED_STORY_MULTIPLIER as i64 });
    check("shuffle_connections", constants, |input| {
        let graph = Graph {
            story_id: int(&input["story_id"]),
            locations: input["locations"]
                .as_array()
                .expect("locations")
                .iter()
                .map(|l| (int(&l[0]), l[1] == true))
                .collect(),
            connections: id_pairs(&input["connections"]),
        };
        let edges = graph.anchor_edges();
        let arrangements: Vec<Value> = input["ats"]
            .as_array()
            .expect("ats")
            .iter()
            .map(|at| json!(graph.choose_arrangement(&edges, int(at))))
            .collect();
        json!({
            "anchor_edges": edges.iter().map(|e| json!([e.0, e.1])).collect::<Vec<_>>(),
            "arrangements": arrangements,
        })
    });
}

/// The story every interior case is laid out in starts at this second.
const INTERIOR_STORY_START: i64 = 1_767_225_600;

#[test]
fn interior() {
    use engine_core::interior::{self, Place};
    let range = |(low, high): (i64, i64)| json!([low, high]);
    let constants = json!({
        "minimum_side": interior::MINIMUM_SIDE,
        "storeys": range(interior::STOREYS),
        "rooms_per_storey": range(interior::ROOMS_PER_STOREY),
        "basements": range(interior::BASEMENTS),
        "footprint_sides": range(interior::FOOTPRINT_SIDES),
        "stairwells": range(interior::STAIRWELLS),
        "door_die": interior::DOOR_DIE,
        "extra_door_share": interior::EXTRA_DOOR_SHARE,
        "paces_per_minute": interior::PACES_PER_MINUTE,
        "max_exits": interior::MAX_EXITS,
        "distances": pairs(&interior::DISTANCES, |n| json!(n)),
    });
    check("interior", constants, |input| {
        let footprint = input["footprint"]
            .as_array()
            .map(|f| (int(&f[0]), int(&f[1])));
        let place = Place {
            story_id: int(&input["story_id"]),
            id: int(&input["place_id"]),
            name: input["place_name"].as_str().expect("a place name"),
            clock: INTERIOR_STORY_START,
            existing_locations: 1,
            footprint,
        };
        let picks = input["picks"]
            .is_object()
            .then(|| picks_of(&input["picks"]));
        let layout = interior::lay_out(&place, input["below"].as_i64(), picks.as_ref());
        json!({
            "footprint": [layout.footprint.0, layout.footprint.1],
            "rooms": layout.rooms.iter().map(|room| json!({
                "name": room.name,
                "x": room.bounds.x, "y": room.bounds.y, "z": room.bounds.z,
                "width": room.bounds.width, "depth": room.bounds.depth,
                "danger": room.danger,
                "hazard": room.hazard.map(|h| h.hazard),
                "hazard_die": room.hazard.map(|h| h.hazard_die),
            })).collect::<Vec<_>>(),
            "edges": layout.edges.iter().map(|e| json!([e.from, e.to, e.distance, e.travel_method])).collect::<Vec<_>>(),
        })
    });
}

#[test]
fn cast() {
    use engine_core::cast::{self, Race};
    let constants = json!({
        "sexes": cast::SEXES,
        "npc_ages": [cast::NPC_AGES.0, cast::NPC_AGES.1],
        "generated_ages": [cast::GENERATED_AGES.0, cast::GENERATED_AGES.1],
        "attractiveness": cast::ATTRACTIVENESS,
        "birth_places": cast::BIRTH_PLACES,
        "raised_by": cast::RAISED_BY,
        "max_per_room": cast::MAX_PER_ROOM,
    });
    check("cast", constants, |input| {
        let races: Vec<Race> = input["races"]
            .as_array()
            .expect("races")
            .iter()
            .map(|r| Race {
                name: r[0].as_str().expect("a race name"),
                monstrous: r[1] == true,
            })
            .collect();
        if let Some(sequence) = input.get("sequence") {
            let person = cast::generated(int(&input["story_id"]), int(sequence), &races);
            return json!({
                "race": person.race, "age": person.age, "sex": person.sex,
                "attractiveness": person.attractiveness, "born_in": person.born_in,
                "raised_by": person.raised_by,
            });
        }
        let room = cast::Room {
            story_id: int(&input["story_id"]),
            location_id: int(&input["location_id"]),
            name: input["name"].as_str().expect("a room name"),
            danger: input["danger"].as_str().expect("a danger"),
            population: input["population"].as_str(),
            present: 0,
            in_story: 0,
        };
        cast::slots(&room, &races)
            .iter()
            .map(|slot| json!({ "race": slot.race, "age": slot.age, "sex": slot.sex }))
            .collect()
    });
}
