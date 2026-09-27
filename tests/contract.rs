//! The protocol types against the published contract (`spec/openapi.json`):
//! every example payload must satisfy the spec's schema and round-trip
//! through the Rust types unchanged, and everything the fake engine serves
//! must satisfy the spec too.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use text_adventure_terminal::engine::fake::FakeEngine;
use text_adventure_terminal::engine::{follow_turn, Engine, Reconnect};
use text_adventure_terminal::protocol::*;

fn spec() -> Value {
    serde_json::from_str(include_str!("../spec/openapi.json")).unwrap()
}

fn validator(schema: &str) -> jsonschema::Validator {
    let mut root = spec();
    root["$ref"] = Value::String(format!("#/components/schemas/{schema}"));
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&root)
        .unwrap()
}

fn assert_valid(schema: &str, instance: &Value) {
    let v = validator(schema);
    let errors: Vec<String> = v.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{schema} does not match the spec: {errors:?}"
    );
}

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn round_trip<T: Serialize + DeserializeOwned>(schema: &str, name: &str) -> T {
    let json = fixture(name);
    assert_valid(schema, &json);
    let typed: T = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&typed).unwrap(),
        json,
        "{name} lost or changed a field through the Rust types"
    );
    typed
}

#[test]
fn examples_match_the_spec_and_round_trip() {
    let service: Service = round_trip("Service", "Service");
    assert_eq!(service.protocol, PROTOCOL);
    assert_eq!(service.version, VERSION);
    round_trip::<WorldList>("WorldList", "WorldList");
    round_trip::<GameList>("GameList", "GameList");
    let screen: Screen = round_trip("Screen", "Screen");
    assert_eq!(
        screen.standing.saved_turn.unwrap().action,
        SavedAction::Resume
    );
    round_trip::<TurnAccepted>("TurnAccepted", "TurnAccepted");
    round_trip::<StandingResponse>("StandingResponse", "StandingResponse");
    round_trip::<StartedEvent>("StartedEvent", "StartedEvent");
    round_trip::<ProseEvent>("ProseEvent", "ProseEvent");
    round_trip::<GlanceEvent>("GlanceEvent", "GlanceEvent");
    let finished: FinishedEvent = round_trip("FinishedEvent", "FinishedEvent");
    assert_eq!(finished.rolls.len(), 3);
    let refused: FinishedEvent = round_trip("FinishedEvent", "FinishedEvent.refused");
    assert_eq!(refused.refusal.unwrap().kind, RefusalKind::Immovable);
    let limit: ErrorBody = round_trip("Error", "Error.limit");
    assert_eq!(limit.error.code, ErrorCode::LimitReached);
    round_trip::<ErrorBody>("Error", "Error.rate");
}

/// A new top-level schema in the spec needs a type and an example here.
#[test]
fn every_schema_in_the_spec_is_covered() {
    let spec = spec();
    let names: BTreeSet<String> = spec["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let covered = [
        "Error",
        "Service",
        "WorldList",
        "GameList",
        "Screen",
        "TurnAccepted",
        "StandingResponse",
        "StartedEvent",
        "ProseEvent",
        "GlanceEvent",
        "FinishedEvent",
        // Reached through the ones above.
        "World",
        "Game",
        "Exit",
        "Person",
        "Thing",
        "Verb",
        "Glance",
        "SavedTurn",
        "Standing",
        "LogEntry",
        "Turn",
    ];
    let covered: BTreeSet<String> = covered.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, covered);
}

#[test]
fn a_missing_required_field_fails_both_the_spec_and_the_types() {
    let mut json = fixture("FinishedEvent");
    json.as_object_mut().unwrap().remove("rolls");
    assert!(!validator("FinishedEvent").is_valid(&json));
    assert!(serde_json::from_value::<FinishedEvent>(json).is_err());
}

#[test]
fn unknown_fields_are_ignored() {
    let mut json = fixture("Service");
    json["added_later"] = Value::Bool(true);
    json["player"]["spend"]["new_field"] = Value::from(1);
    assert!(serde_json::from_value::<Service>(json).is_ok());
}

#[test]
fn the_fake_engine_serves_the_spec() {
    let engine = FakeEngine::new();
    let v = |schema: &str, value: Value| assert_valid(schema, &value);
    v(
        "Service",
        serde_json::to_value(engine.service().unwrap()).unwrap(),
    );
    v(
        "WorldList",
        serde_json::to_value(WorldList {
            worlds: engine.worlds().unwrap(),
        })
        .unwrap(),
    );
    v(
        "GameList",
        serde_json::to_value(GameList {
            games: engine.games().unwrap(),
        })
        .unwrap(),
    );
    v(
        "Screen",
        serde_json::to_value(engine.screen("game-1").unwrap()).unwrap(),
    );
    let turn = engine
        .send_turn("game-1", "/go The Sea Stair", "rt-1")
        .unwrap();
    v(
        "TurnAccepted",
        serde_json::to_value(TurnAccepted { turn: turn.clone() }).unwrap(),
    );
    follow_turn(&engine, "game-1", &turn.id, &Reconnect::default(), |p| {
        if let text_adventure_terminal::engine::Progress::Event(e) = p {
            let schema = match e.kind {
                TurnEventKind::Started(_) => "StartedEvent",
                TurnEventKind::Prose(_) => "ProseEvent",
                TurnEventKind::Glance(_) => "GlanceEvent",
                TurnEventKind::Finished(_) => "FinishedEvent",
            };
            assert_valid(schema, &serde_json::from_str(&e.kind.data_json()).unwrap());
        }
    })
    .unwrap();
}
