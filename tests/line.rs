//! Plain line mode against the fake engine.

use std::time::Duration;

use text_adventure_terminal::engine::fake::{sample_screen, FakeEngine};
use text_adventure_terminal::engine::{EngineError, Reconnect};
use text_adventure_terminal::line::{run, Options};
use text_adventure_terminal::protocol::ErrorCode;

fn play(engine: &FakeEngine, input: &str, json: bool) -> (String, usize) {
    let mut out = Vec::new();
    let policy = Reconnect {
        attempts: 3,
        backoff: Duration::ZERO,
    };
    let errors = run(
        engine,
        &sample_screen(),
        input.as_bytes(),
        &mut out,
        Options { json },
        &policy,
    )
    .unwrap();
    (String::from_utf8(out).unwrap(), errors)
}

#[test]
fn writes_saved_prose_and_outcomes() {
    let engine = FakeEngine::new();
    let (out, errors) = play(&engine, "wave\n\n/go The Sea Stair\n", false);
    assert_eq!(errors, 0);
    assert_eq!(
        out,
        "The chalk is still under your thumb and it is still not your handwriting.\n\n\
         Justicar Brace waits on the bench.\n\n\
         You try to wave. The fog does not care either way.\n\
         » narrated\n\n\
         You try to /go The Sea Stair. The fog does not care either way.\n\
         » narrated\n\n"
    );
    assert!(!out.contains('\u{1b}'), "no terminal control codes");
}

#[test]
fn errors_are_written_in_the_engines_words_and_counted() {
    let engine = FakeEngine::new();
    engine.fail_next_send(EngineError::Api {
        status: 402,
        code: ErrorCode::LimitReached,
        message: "You have reached this month's spend limit of $5.00.".into(),
    });
    let (out, errors) = play(&engine, "wave\nwave\n", false);
    assert_eq!(errors, 1);
    assert!(out.contains("! You have reached this month's spend limit of $5.00.\n"));
    assert!(out.contains("You try to wave."));
}

#[test]
fn a_bad_token_stops_the_run() {
    let engine = FakeEngine::new();
    engine.fail_next_send(EngineError::Api {
        status: 401,
        code: ErrorCode::Unauthorized,
        message: "That token is not recognised.".into(),
    });
    let (out, errors) = play(&engine, "wave\nwave\n", true);
    assert_eq!(errors, 1);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 1, "stopped after the 401: {out}");
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["error"]["code"], "unauthorized");
    assert_eq!(v["error"]["message"], "That token is not recognised.");
}

#[test]
fn json_mode_writes_one_finished_event_per_line() {
    let engine = FakeEngine::new();
    engine.drop_streams(1, 1);
    let (out, _) = play(&engine, "wave\n", true);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["outcome"]["kind"], "narrated");
    assert_eq!(engine.connects(), vec![None, Some("1".into())]);
}
