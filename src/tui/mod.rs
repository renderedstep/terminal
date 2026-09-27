//! The full-screen client.

pub mod app;
pub mod view;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event};

use crate::engine::{follow_turn, Engine, Reconnect};
use app::{App, Command, EngineMsg};

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

pub fn run(engine: Arc<dyn Engine>, game: String, mut app: App) -> anyhow::Result<()> {
    let (tx, rx): (Sender<EngineMsg>, Receiver<EngineMsg>) = mpsc::channel();
    let mut terminal = ratatui::init();
    let result = (|| -> anyhow::Result<()> {
        while !app.quit {
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
        Ok(())
    })();
    ratatui::restore();
    result
}
