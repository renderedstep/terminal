//! The protocol types against the published contract (`spec/openapi.json`):
//! every example the spec carries must satisfy its schema and round-trip
//! through the Rust types unchanged, and everything the fake engine serves
//! must satisfy the spec too.

mod common;

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use text_adventure_terminal::engine::fake::FakeEngine;
use text_adventure_terminal::engine::{follow_turn, Engine, Reconnect};
use text_adventure_terminal::protocol::*;
use text_adventure_terminal::sse::SseReader;

fn validator_for(schema: Value) -> jsonschema::Validator {
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .unwrap()
}

fn validator(schema: &str) -> jsonschema::Validator {
    let mut root = common::spec();
    root["$ref"] = Value::String(format!("#/components/schemas/{schema}"));
    validator_for(root)
}

fn assert_valid(schema: &str, instance: &Value) {
    let v = validator(schema);
    let errors: Vec<String> = v.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{schema} does not match the spec: {errors:?}"
    );
}

fn round_trip<T: Serialize + DeserializeOwned>(what: &str, json: &Value) -> T {
    let typed: T = serde_json::from_value(json.clone())
        .unwrap_or_else(|e| panic!("{what} does not read into the Rust types: {e}"));
    assert_eq!(
        &serde_json::to_value(&typed).unwrap(),
        json,
        "{what} lost or changed a field through the Rust types"
    );
    typed
}

/// Check one example against its schema and its Rust type.
fn check(what: &str, schema: &str, json: &Value) {
    assert_valid(schema, json);
    match schema {
        "Error" => {
            round_trip::<ErrorBody>(what, json);
        }
        "Service" => {
            round_trip::<Service>(what, json);
        }
        "WorldList" => {
            round_trip::<WorldList>(what, json);
        }
        "GameList" => {
            round_trip::<GameList>(what, json);
        }
        "Screen" => {
            round_trip::<Screen>(what, json);
        }
        "TurnAccepted" => {
            round_trip::<TurnAccepted>(what, json);
        }
        "StandingResponse" => {
            round_trip::<StandingResponse>(what, json);
        }
        "StartedEvent" => {
            round_trip::<StartedEvent>(what, json);
        }
        "ProseEvent" => {
            round_trip::<ProseEvent>(what, json);
        }
        "GlanceEvent" => {
            round_trip::<GlanceEvent>(what, json);
        }
        "FinishedEvent" => {
            round_trip::<FinishedEvent>(what, json);
        }
        other => panic!("{what}: no Rust type for {other}"),
    }
}

#[test]
fn every_response_example_matches_the_spec_and_round_trips() {
    let examples = common::all_responses();
    assert!(examples.len() > 20, "the spec lost its examples");
    for (endpoint, status, schema, body) in &examples {
        check(&format!("{endpoint} {status}"), schema, body);
    }
    let service: Service =
        serde_json::from_value(common::response_body("/api/v1", "get", "200")).unwrap();
    assert_eq!(service.protocol, PROTOCOL);
    assert_eq!(service.version, VERSION);
}

#[test]
fn every_event_example_matches_the_spec_and_round_trips() {
    let spec = common::spec();
    for kind in ["StartedEvent", "ProseEvent", "GlanceEvent", "FinishedEvent"] {
        let examples = spec["components"]["schemas"][kind]["examples"]
            .as_array()
            .unwrap();
        assert!(!examples.is_empty(), "{kind} has no example");
        for json in examples {
            check(kind, kind, json);
        }
    }
}

#[test]
fn the_example_stream_decodes_event_by_event() {
    let stream = common::event_stream();
    let mut reader = SseReader::new(stream.as_bytes());
    let mut names = vec![];
    while let Some(raw) = reader.next_event().unwrap() {
        let kind = TurnEventKind::decode(&raw.event, &raw.data)
            .expect("a v1 event name")
            .unwrap();
        let schema = match kind {
            TurnEventKind::Started(_) => "StartedEvent",
            TurnEventKind::Prose(_) => "ProseEvent",
            TurnEventKind::Glance(_) => "GlanceEvent",
            TurnEventKind::Finished(_) => "FinishedEvent",
        };
        check(
            &format!("stream event {:?}", raw.id),
            schema,
            &serde_json::from_str(&raw.data).unwrap(),
        );
        names.push(kind.name());
    }
    assert_eq!(names, ["started", "prose", "glance", "finished"]);
}

#[test]
fn request_examples_match_the_spec_and_the_types() {
    for (path, want) in [
        ("/api/v1/games", "NewGameRequest"),
        ("/api/v1/games/{game}/turns", "TurnRequest"),
    ] {
        let spec = common::spec();
        let schema = spec["paths"][path]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]
            .clone();
        let json = common::request_body(path, "post");
        assert!(validator_for(schema).is_valid(&json), "{path} request");
        match want {
            "NewGameRequest" => {
                round_trip::<NewGameRequest>(path, &json);
            }
            _ => {
                round_trip::<TurnRequest>(path, &json);
            }
        }
    }
}

#[test]
fn revision_1_1_fields_are_read() {
    let screen: Screen =
        serde_json::from_value(common::response_body("/api/v1/games/{game}", "get", "200"))
            .unwrap();
    let go = screen.glance.verb(VerbName::Move).unwrap();
    assert_eq!(go.word.as_deref(), Some("go"));
    assert_eq!(go.lines, None);
    let use_verb = screen.glance.verb(VerbName::Use).unwrap();
    assert_eq!(use_verb.word, None);
    assert_eq!(
        use_verb.lines.as_ref().map(Vec::len),
        Some(use_verb.targets.len()),
        "one line per use target"
    );
    assert!(screen.standing.busy);
    assert!(screen.standing.running_turn.is_some());
}

#[test]
fn a_revision_1_0_payload_still_reads() {
    let mut json = common::response_body("/api/v1/games/{game}", "get", "200");
    json["standing"]
        .as_object_mut()
        .unwrap()
        .remove("running_turn");
    for verb in json["glance"]["verbs"].as_array_mut().unwrap() {
        let verb = verb.as_object_mut().unwrap();
        verb.remove("word");
        verb.remove("lines");
    }
    let screen: Screen = serde_json::from_value(json).unwrap();
    assert!(screen.glance.verbs.iter().all(|v| v.word.is_none()));
    assert_eq!(screen.standing.running_turn, None);
}

/// A new top-level schema in the spec needs a type here.
#[test]
fn every_schema_in_the_spec_is_covered() {
    let spec = common::spec();
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

fn finished_example() -> Value {
    common::spec()["components"]["schemas"]["FinishedEvent"]["examples"][0].clone()
}

#[test]
fn a_missing_required_field_fails_both_the_spec_and_the_types() {
    let mut json = finished_example();
    json.as_object_mut().unwrap().remove("rolls");
    assert!(!validator("FinishedEvent").is_valid(&json));
    assert!(serde_json::from_value::<FinishedEvent>(json).is_err());
}

#[test]
fn unknown_fields_are_ignored() {
    let mut json = common::response_body("/api/v1", "get", "200");
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
    engine.leave_running("/take Filing press");
    v(
        "Screen",
        serde_json::to_value(engine.screen("game-1").unwrap()).unwrap(),
    );
}
