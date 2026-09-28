//! The TUI's screens, drawn into a test terminal and compared with
//! snapshots, and its flow driven against the fake engine. The start screen
//! and `--resume` too.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use text_adventure_terminal::engine::fake::{
    sample_glance, sample_screen, sample_standing, sample_world, FakeEngine, ScriptedTurn,
};
use text_adventure_terminal::engine::{Engine, EngineError};
use text_adventure_terminal::protocol::*;
use text_adventure_terminal::tui::app::{App, Command, EngineMsg, Entry};
use text_adventure_terminal::tui::start::{latest_unfinished, Choice, Start};
use text_adventure_terminal::tui::{self, execute, view, Opening};

fn render(app: &App, width: u16, height: u16) -> String {
    draw_text(width, height, |f| view::draw(f, app))
}

fn draw_text(width: u16, height: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(draw).unwrap();
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

/// The sample screen in a room with more ways out than a short terminal fits.
fn crowded_app() -> App {
    let mut screen = sample_screen();
    screen.glance.exits = [
        "The Causeway Court",
        "The Sea Stair",
        "The Bell Loft",
        "The Toll Gate",
        "The Drowned Cellar",
        "The Clerks' Passage",
        "The Signal Walk",
        "The Watch Room",
        "The Lamp Room",
        "The Counting House",
        "The Weir",
        "The Harbour Steps",
    ]
    .iter()
    .map(|name| Exit {
        name: name.to_string(),
        written: true,
        open: true,
    })
    .collect();
    App::new(screen, "Wren · fake 0.0.0".into())
}

#[test]
fn a_short_terminal_keeps_ways_out_and_marks_hidden_rows() {
    let screen = render(&crowded_app(), 110, 20);
    assert!(
        screen.contains("The Harbour Steps"),
        "every exit fits at 20 rows"
    );
    assert!(screen.contains("+5 hidden panels · :panels"));
    insta::assert_snapshot!(screen);
}

#[test]
fn a_very_short_terminal_marks_hidden_exits() {
    let screen = render(&crowded_app(), 110, 14);
    assert!(screen.contains("The Tide Post +1 more"));
    assert!(screen.contains("+5 more · :exits"));
    insta::assert_snapshot!(screen);
}

#[test]
fn ways_out_gets_height_first() {
    let titles = [
        "Room",
        "Ways out",
        "Here",
        "Lying here",
        "Carrying",
        "Condition",
        "Next beat",
    ];
    let needed = [4, 10, 4, 3, 3, 4, 3];
    // Ways out takes all it needs, then the others get a row each in turn.
    assert_eq!(
        view::panel_heights(&titles, &needed, 24),
        vec![4, 10, 4, 3, 3, 0, 0]
    );
    // Too short even for every exit: Ways out still comes first.
    assert_eq!(
        view::panel_heights(&titles, &needed, 8),
        vec![3, 5, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        view::panel_heights(&titles, &needed, 31),
        vec![4, 10, 4, 3, 3, 4, 3]
    );
}

#[test]
fn listing_commands_print_full_lists_into_the_log() {
    let mut app = crowded_app();
    for (cmd, expect) in [
        (
            ":exits",
            "Ways out:\n  The Causeway Court\n  The Sea Stair\n  The Bell Loft",
        ),
        (":here", "Here:\n  Neb Halloran\n  The Tollman ⚔ (wounded)"),
        (":items", "Lying here:\n  Filing press"),
        (":carrying", "Carrying:\n  Assize tide-slate"),
    ] {
        type_line(&mut app, cmd);
        assert_eq!(app.on_key(key(KeyCode::Enter)), None, "{cmd} sends nothing");
        assert!(app.input.is_empty());
        match app.log.last() {
            Some(Entry::Notice(text)) => assert!(text.starts_with(expect), "{cmd}: {text}"),
            other => panic!("{cmd}: {other:?}"),
        }
    }
    let Some(Entry::Notice(exits)) = app.log.iter().rev().nth(3) else {
        panic!("no :exits listing");
    };
    assert!(exits.ends_with("The Harbour Steps"));

    type_line(&mut app, ":panels");
    app.on_key(key(KeyCode::Enter));
    let Some(Entry::Notice(all)) = app.log.last() else {
        panic!("no :panels listing");
    };
    for part in [
        "Room: The Tide Post (in Gallows Rock)",
        "The Harbour Steps",
        "Carrying:",
        "HP 14/18 · AC 12",
        "Next beat: Find out who wrote on the slate.",
    ] {
        assert!(all.contains(part), "{part} missing from {all}");
    }

    type_line(&mut app, ":help");
    app.on_key(key(KeyCode::Enter));
    let Some(Entry::Notice(help)) = app.log.last() else {
        panic!("no help");
    };
    for cmd in [":exits", ":here", ":items", ":carrying", ":panels"] {
        assert!(help.contains(cmd));
    }
}

#[test]
fn listing_commands_work_while_a_turn_runs() {
    let mut app = crowded_app();
    app.busy = true;
    type_line(&mut app, ":exits");
    app.on_key(key(KeyCode::Enter));
    assert!(matches!(app.log.last(), Some(Entry::Notice(t)) if t.starts_with("Ways out:")));
}

fn render_start(start: &Start, width: u16, height: u16) -> String {
    draw_text(width, height, |f| view::draw_start(f, start))
}

fn game(id: &str, world: World, over: bool, started_at: &str) -> Game {
    Game {
        id: id.into(),
        world,
        over,
        started_at: started_at.into(),
    }
}

fn causeway() -> World {
    World {
        id: "world-causeway".into(),
        title: "The Causeway".into(),
        summary: "A drowned road, walked at low tide.".into(),
    }
}

/// Three games, given oldest first: one over, one unfinished, and the
/// unfinished one started last.
fn sample_games() -> Vec<Game> {
    vec![
        game("game-0", causeway(), true, "2026-09-01T09:00:00Z"),
        game("game-1", sample_world(), false, "2026-09-20T18:00:00Z"),
        game("game-2", causeway(), false, "2026-09-25T07:30:00Z"),
    ]
}

fn start() -> Start {
    Start::new(
        sample_games(),
        vec![sample_world(), causeway()],
        "Wren · fake 0.0.0".into(),
    )
}

#[test]
fn the_start_screen_lists_games_then_worlds() {
    let screen = render_start(&start(), 80, 20);
    let continue_at = screen.find("Continue").unwrap();
    let new_at = screen.find("New game").unwrap();
    assert!(continue_at < new_at);
    insta::assert_snapshot!(screen);
}

#[test]
fn the_start_screen_offers_unfinished_games_first_latest_first() {
    let start = start();
    let ids: Vec<&str> = start.games.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["game-2", "game-1", "game-0"]);
    assert_eq!(start.choice(), Some(Choice::Continue("game-2".into())));
}

#[test]
fn the_start_screen_moves_through_games_into_worlds() {
    let mut start = start();
    assert_eq!(start.on_key(key(KeyCode::Up)), None);
    assert_eq!(start.selected, 0, "Up at the top stays put");
    start.on_key(key(KeyCode::Down));
    start.on_key(key(KeyCode::Char('j')));
    assert_eq!(start.choice(), Some(Choice::Continue("game-0".into())));
    start.on_key(key(KeyCode::Down));
    assert_eq!(
        start.on_key(key(KeyCode::Enter)),
        Some(Choice::New("world-fog".into()))
    );
    start.on_key(key(KeyCode::End));
    start.on_key(key(KeyCode::Down));
    assert_eq!(start.choice(), Some(Choice::New("world-causeway".into())));
    start.on_key(key(KeyCode::Char('k')));
    start.on_key(key(KeyCode::Home));
    assert_eq!(start.choice(), Some(Choice::Continue("game-2".into())));
    assert!(!start.quit);
    start.on_key(key(KeyCode::Esc));
    assert!(start.quit);
}

#[test]
fn an_empty_start_screen_picks_nothing() {
    let mut start = Start::new(vec![], vec![], String::new());
    assert_eq!(start.on_key(key(KeyCode::Down)), None);
    assert_eq!(start.on_key(key(KeyCode::Enter)), None);
    let screen = render_start(&start, 80, 10);
    assert!(screen.contains("No games yet."));
    assert!(screen.contains("The engine offers no worlds."));
}

#[test]
fn resume_picks_the_unfinished_game_started_last() {
    let games = sample_games();
    assert_eq!(latest_unfinished(&games).unwrap().id, "game-2");

    // A finished game is never resumed, however recent.
    let mut games = sample_games();
    games.push(game("game-3", causeway(), true, "2026-09-27T12:00:00Z"));
    assert_eq!(latest_unfinished(&games).unwrap().id, "game-2");

    games.retain(|g| g.over);
    assert_eq!(latest_unfinished(&games), None);
    assert_eq!(latest_unfinished(&[]), None);
}

#[test]
fn resume_opens_the_game_without_the_start_screen() {
    let engine = FakeEngine::new();
    let Opening::Game(screen) = tui::opening(&engine, true, "label").unwrap() else {
        panic!("--resume showed the start screen");
    };
    assert_eq!(screen.game.id, "game-1");
}

#[test]
fn resume_with_nothing_to_resume_falls_back_to_the_start_screen() {
    let engine = FakeEngine::new();
    let mut over = sample_screen();
    over.game.over = true;
    engine.set_screen(over);
    let Opening::Choose(start) = tui::opening(&engine, true, "label").unwrap() else {
        panic!("resumed a finished game");
    };
    assert_eq!(start.status, "You have no unfinished game to resume.");
    assert_eq!(start.choice(), Some(Choice::Continue("game-1".into())));
}

#[test]
fn plain_ta_opens_on_the_start_screen() {
    let engine = FakeEngine::new();
    let Opening::Choose(start) = tui::opening(&engine, false, "label").unwrap() else {
        panic!("opened a game without asking");
    };
    assert!(start.status.is_empty());
    assert_eq!(start.games.len(), 1);
    assert_eq!(start.worlds, vec![sample_world()]);
}

#[test]
fn a_start_screen_choice_opens_its_game_or_starts_one() {
    let engine = FakeEngine::new();
    let screen = tui::open_choice(&engine, &Choice::Continue("game-1".into())).unwrap();
    assert_eq!(screen.game.id, "game-1");
    let screen = tui::open_choice(&engine, &Choice::New("world-fog".into())).unwrap();
    assert_eq!(screen.game.world, sample_world());
    let refused = tui::open_choice(&engine, &Choice::New("world-causeway".into())).unwrap_err();
    assert_eq!(refused.code(), Some(ErrorCode::Unplayable));
}

#[test]
fn a_short_start_screen_keeps_the_choice_in_view() {
    let mut start = start();
    start.on_key(key(KeyCode::End));
    let screen = render_start(&start, 80, 8);
    assert!(screen.contains("> The Causeway "), "{screen}");
    assert!(screen.contains("A drowned road, walked at low tide."));
}
