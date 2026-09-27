//! The TUI's screens, drawn into a test terminal and compared with
//! snapshots, and its flow driven against the fake engine.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use text_adventure_terminal::engine::fake::{
    sample_glance, sample_screen, sample_standing, FakeEngine, ScriptedTurn,
};
use text_adventure_terminal::engine::{Engine, EngineError};
use text_adventure_terminal::protocol::*;
use text_adventure_terminal::tui::app::{App, Command, EngineMsg, Entry};
use text_adventure_terminal::tui::{execute, view};

fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| view::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn app() -> App {
    App::new(sample_screen(), "Wren · fake 0.0.0".into())
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_line(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

/// Run commands against the engine until the app has nothing left to do.
fn drive(app: &mut App, engine: Arc<dyn Engine>, first: Command) {
    let (tx, rx) = mpsc::channel();
    execute(engine.clone(), "game-1".into(), first, tx.clone());
    let mut outstanding = 1;
    while outstanding > 0 {
        let msg = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("engine replied");
        let terminal = !matches!(msg, EngineMsg::Progress(_));
        if terminal {
            outstanding -= 1;
        }
        if let Some(cmd) = app.on_engine(msg) {
            outstanding += 1;
            execute(engine.clone(), "game-1".into(), cmd, tx.clone());
        }
    }
}

#[test]
fn wide_screen_shows_every_panel() {
    insta::assert_snapshot!(render(&app(), 110, 34));
}

#[test]
fn narrow_screen_folds_the_panels() {
    insta::assert_snapshot!(render(&app(), 70, 22));
}

#[test]
fn streaming_prose_shows_as_a_preview() {
    let mut app = app();
    type_line(&mut app, "/take Filing press");
    let cmd = app.on_key(key(KeyCode::Enter));
    assert!(matches!(cmd, Some(Command::Send { .. })));
    app.on_engine(EngineMsg::Progress(
        text_adventure_terminal::engine::Progress::Event(TurnEvent {
            id: "2".into(),
            kind: TurnEventKind::Prose(ProseEvent {
                turn: "t1".into(),
                text: "You put your shoulder to the press".into(),
            }),
        }),
    ));
    insta::assert_snapshot!(render(&app, 110, 24));
}

fn finished(
    kind: OutcomeKind,
    text: &str,
    rolls: Vec<Roll>,
    refusal: Option<Refusal>,
) -> FinishedEvent {
    FinishedEvent {
        turn: String::new(),
        outcome: Outcome { kind },
        text: Some(text.into()),
        notices: vec![],
        glance: sample_glance(),
        standing: sample_standing(),
        refusal,
        resolved_by: Some("grammar".into()),
        rolls,
    }
}

#[test]
fn a_finished_turn_shows_its_outcome_and_dice() {
    let engine = Arc::new(FakeEngine::new());
    engine.script(ScriptedTurn {
        prose: vec!["You swing".into(), " at the Tollman.".into()],
        glance: None,
        finished: finished(
            OutcomeKind::Attacked,
            "You swing at the Tollman and he reels back against the rail.",
            vec![
                Roll {
                    kind: RollKind::Check,
                    die: Some(20),
                    result: Some(15),
                    target: Some(12),
                },
                Roll {
                    kind: RollKind::Blow,
                    die: None,
                    result: Some(4),
                    target: None,
                },
            ],
            None,
        ),
    });
    let mut app = app();
    type_line(&mut app, "/att");
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.input, "/attack ");
    app.on_key(key(KeyCode::Tab));
    // A second Tab with one choice keeps it.
    assert_eq!(app.input, "/attack ");
    type_line(&mut app, "T");
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.input, "/attack The Tollman");
    let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
    drive(&mut app, engine.clone(), cmd);
    assert!(!app.busy);
    assert!(
        app.preview.is_empty(),
        "the saved text replaces the preview"
    );
    insta::assert_snapshot!(render(&app, 110, 24));
}

#[test]
fn a_refusal_is_shown_in_the_engines_words() {
    let engine = Arc::new(FakeEngine::new());
    let words = "The filing press is bolted to the floor.";
    engine.script(ScriptedTurn {
        prose: vec![],
        glance: None,
        finished: finished(
            OutcomeKind::Refused,
            words,
            vec![],
            Some(Refusal {
                kind: RefusalKind::Immovable,
                text: words.into(),
            }),
        ),
    });
    let mut app = app();
    type_line(&mut app, "/take Filing press");
    let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
    drive(&mut app, engine, cmd);
    insta::assert_snapshot!(render(&app, 110, 20));
}

#[test]
fn spend_limit_rate_limit_and_bad_token_are_shown_in_the_engines_words() {
    let engine = Arc::new(FakeEngine::new());
    engine.fail_next_send(EngineError::Api {
        status: 402,
        code: ErrorCode::LimitReached,
        message: "You have reached this month's spend limit of $5.00.".into(),
    });
    engine.fail_next_send(EngineError::Api {
        status: 429,
        code: ErrorCode::RateLimited,
        message: "Too many turns this minute.".into(),
    });
    engine.fail_next_send(EngineError::Api {
        status: 401,
        code: ErrorCode::Unauthorized,
        message: "That token is not recognised.".into(),
    });
    let mut app = app();
    for _ in 0..3 {
        type_line(&mut app, "/go The Sea Stair");
        let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
        drive(&mut app, engine.clone(), cmd);
    }
    insta::assert_snapshot!(render(&app, 110, 34));
}

#[test]
fn a_saved_turn_is_offered_and_resumed_with_its_token() {
    let engine = Arc::new(FakeEngine::new());
    let mut screen = sample_screen();
    screen.standing.saved_turn = Some(SavedTurn {
        turn: "turn-9".into(),
        line: "/go The Sea Stair".into(),
        action: SavedAction::Resume,
        request_token: "saved-token".into(),
    });
    engine.set_screen(screen.clone());
    let mut app = App::new(screen, String::new());
    assert!(render(&app, 110, 24).contains("Type :resume to finish it."));
    type_line(&mut app, ":resume");
    let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
    assert_eq!(
        cmd,
        Command::Send {
            line: "/go The Sea Stair".into(),
            request_token: "saved-token".into()
        }
    );
    drive(&mut app, engine.clone(), cmd);
    assert_eq!(
        engine.sent_lines(),
        vec![("/go The Sea Stair".into(), "saved-token".into())]
    );
}

#[test]
fn a_dropped_stream_is_resumed_without_repeating_prose() {
    let engine = Arc::new(FakeEngine::new());
    engine.drop_streams(2, 1);
    let mut app = app();
    type_line(&mut app, "wave");
    let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
    drive(&mut app, engine.clone(), cmd);
    assert_eq!(engine.connects(), vec![None, Some("2".into())]);
    let prose: Vec<_> = app
        .log
        .iter()
        .filter(|e| matches!(e, text_adventure_terminal::tui::app::Entry::Prose(t) if t.contains("wave")))
        .collect();
    assert_eq!(prose.len(), 1);
}

#[test]
fn reopening_a_busy_game_follows_the_running_turn() {
    let engine = Arc::new(FakeEngine::new());
    let running = engine.leave_running("/take Filing press");
    let mut app = App::new(engine.screen("game-1").unwrap(), "Wren".into());
    let first = app.start().expect("a running turn is followed");
    assert_eq!(first, Command::Follow { turn: running });
    assert!(app.busy);
    drive(&mut app, engine.clone(), first);
    assert!(!app.busy);
    assert!(app.log.contains(&Entry::Prose(
        "You try to /take Filing press. The fog does not care either way.".into()
    )));
    assert_eq!(engine.sent_lines().len(), 1, "nothing is sent again");
}

#[test]
fn a_use_target_is_sent_as_its_line() {
    let mut app = app();
    type_line(&mut app, "/use off");
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.input, "/offer Assize tide-slate to Neb Halloran");
    app.input = "/use Offer Assize tide-slate to Neb Halloran; they may refuse".into();
    let cmd = app.on_key(key(KeyCode::Enter)).unwrap();
    assert!(matches!(
        cmd,
        Command::Send { line, .. } if line == "/offer Assize tide-slate to Neb Halloran"
    ));
}
