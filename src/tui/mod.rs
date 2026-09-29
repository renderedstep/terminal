//! The full-screen client, and the start screen it opens on.

pub mod app;
pub mod start;
pub mod view;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::engine::{self, follow_turn, Engine, Reconnect};
use crate::present;
use crate::protocol::Screen;
use app::{App, Command, EngineMsg};
use start::{latest_unfinished, Choice, Start};

/// Carry out one command on a worker thread, reporting back on `tx`.
pub fn execute(engine: Arc<dyn Engine>, game: String, command: Command, tx: Sender<EngineMsg>) {
    std::thread::spawn(move || {
        let msg = match command {
            Command::Send {
                line,
                request_token,
            } => match engine.send_turn(&game, &line, &request_token) {
                Ok(turn) => EngineMsg::Accepted(turn),
                Err(e) => EngineMsg::Failed(e),
            },
            Command::Follow { turn } => {
                let progress_tx = tx.clone();
                match follow_turn(engine.as_ref(), &game, &turn, &Reconnect::default(), |p| {
                    let _ = progress_tx.send(EngineMsg::Progress(p));
                }) {
                    Ok(f) => EngineMsg::Finished(Box::new(f)),
                    Err(e) => EngineMsg::Failed(e),
                }
            }
            Command::Acknowledge { turn } => match engine.acknowledge(&game, &turn) {
                Ok(s) => EngineMsg::Standing(s),
                Err(e) => EngineMsg::Failed(e),
            },
            Command::Refresh => match engine.screen(&game) {
                Ok(s) => EngineMsg::Screen(Box::new(s)),
                Err(e) => EngineMsg::Failed(e),
            },
        };
        let _ = tx.send(msg);
    });
}

/// What the full-screen client opens on.
pub enum Opening {
    /// Straight into this game.
    Game(Box<Screen>),
    /// The start screen, to pick a game or a world first.
    Choose(Start),
}

/// What plain `ta` opens on: the start screen, or with `resume` the
/// unfinished game started last. With none to resume, the start screen says so.
pub fn opening(engine: &dyn Engine, resume: bool, engine_label: &str) -> engine::Result<Opening> {
    let games = engine.games()?;
    if resume {
        if let Some(game) = latest_unfinished(&games) {
            return Ok(Opening::Game(Box::new(engine.screen(&game.id)?)));
        }
    }
    let mut start = Start::new(games, engine.worlds()?, engine_label.to_string());
    if resume {
        start.status = "You have no unfinished game to resume.".into();
    }
    Ok(Opening::Choose(start))
}

/// The start screen again, fresh from the engine, after the player leaves a
/// game with Esc or /menu.
pub fn back_to_menu(engine: &dyn Engine, engine_label: &str) -> engine::Result<Opening> {
    opening(engine, false, engine_label)
}

/// Open what the player picked on the start screen.
pub fn open_choice(engine: &dyn Engine, choice: &Choice) -> engine::Result<Screen> {
    match choice {
        Choice::Continue(game) => engine.screen(game),
        Choice::New(world) => engine.start_game(world),
    }
}

pub fn run(engine: Arc<dyn Engine>, opening: Opening, engine_label: String) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let result = (|| -> anyhow::Result<()> {
        let mut opening = opening;
        loop {
            let screen = match opening {
                Opening::Game(screen) => *screen,
                Opening::Choose(mut start) => {
                    match choose(&mut terminal, engine.as_ref(), &mut start)? {
                        Some(screen) => screen,
                        None => return Ok(()),
                    }
                }
            };
            let app = play(
                &mut terminal,
                engine.clone(),
                App::new(screen, engine_label.clone()),
            )?;
            if !app.menu {
                return Ok(());
            }
            opening = back_to_menu(engine.as_ref(), &engine_label)?;
        }
    })();
    ratatui::restore();
    result
}

/// Run the start screen until the player opens a game, or quits (`None`).
/// A choice the engine refuses is shown in its words, and the player picks
/// again.
fn choose(
    terminal: &mut DefaultTerminal,
    engine: &dyn Engine,
    start: &mut Start,
) -> anyhow::Result<Option<Screen>> {
    while !start.quit {
        terminal.draw(|f| view::draw_start(f, start))?;
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        let Some(choice) = start.on_key(key) else {
            continue;
        };
        terminal.draw(|f| view::draw_start(f, start))?;
        match open_choice(engine, &choice) {
            Ok(screen) => return Ok(Some(screen)),
            Err(e) => start.status = present::error_lines(&e).0,
        }
    }
    Ok(None)
}

fn play(
    terminal: &mut DefaultTerminal,
    engine: Arc<dyn Engine>,
    mut app: App,
) -> anyhow::Result<App> {
    let game = app.game.clone();
    let (tx, rx): (Sender<EngineMsg>, Receiver<EngineMsg>) = mpsc::channel();
    if let Some(cmd) = app.start() {
        execute(engine.clone(), game.clone(), cmd, tx.clone());
    }
    while !app.quit && !app.menu {
        terminal.draw(|f| view::draw(f, &app))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(cmd) = app.on_key(key) {
                    execute(engine.clone(), game.clone(), cmd, tx.clone());
                }
            }
        }
        while let Ok(msg) = rx.try_recv() {
            if let Some(cmd) = app.on_engine(msg) {
                execute(engine.clone(), game.clone(), cmd, tx.clone());
            }
        }
    }
    Ok(app)
}
