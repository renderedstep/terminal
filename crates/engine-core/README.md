# engine-core

The text-adventure engine's pure rules in Rust: the dice and the geometry.
Every function takes values and returns values. Nothing here touches a
database, the network, the clock or an async runtime, and the crate has no
runtime dependencies.

It reproduces the Ruby engine exactly, roll for roll, and is checked against
the golden vectors that engine exports (`vectors/`, see its README).

| Module | Ruby original | What it answers |
| --- | --- | --- |
| `random` | Ruby's `Random` | MT19937 seeded as `Random.new` seeds it; bounded draws and `Array#shuffle` |
| `roll` | `Roll` | the five-part seed, a die, a pool, one of a list, a weighted pick |
| `stat_block` | `Character::StatBlock` | a body's level, hit die and abilities |
| `spot` | `Location::Spot`, `Location::Placement` | where in a room a thing or a person stands |
| `population` | `Location::Population`, `WorldSeed.natural_key` | how many people a room holds, seeded from a CRC32 of its name |
| `text` | Ruby string behaviour | `downcase` without the final-sigma rule, ASCII `strip` and `\s`, CRC32 |
| `danger` | `Location::Danger` (the pure half) | a new room's danger, monstrous throws, a building's danger and hazards |
| `parameters` | `Location::Parameters` | a building's picks resolved against its tables, kept in key order |
| `boxes` | `Location::Box` | walls, shared ground, bearings and distances between rooms |
| `interior` | `Location::Interior` | a building's rooms and doorways, returned rather than written |
| `shuffle_connections` | `WorldMechanic::ShuffleConnections` | how a shuffle rearranges the mobile rooms' doorways |
| `world_mechanic` | `WorldMechanic` | the cadence boundaries a mechanic has to run for |
| `deadline` | `Quest::Deadline` | the hops walk and the anchor room an overdue step is placed from |
| `cast` | `Character::Registry`, `Character::Generator` | the seeded race, age, sex and background of new people |

## Rules for changing it

- Draw only from `random::Random`. Any other generator rolls different dice.
- Ruby's integer division floors: use `div_euclid`, never `/`, on anything
  that can be negative.
- Tables that Ruby keeps in a `Hash` are arrays of pairs here, so their order
  survives.

## Testing

```sh
cargo test -p engine-core -- --nocapture
```

`tests/vectors/` runs every case of every vector file and prints a pass count
per portion. It also checks each file's `constants` against this crate's
tables, and refuses a file whose format version it does not know.

## Licence

MIT OR Apache-2.0, at your option, as the rest of this repository is.
