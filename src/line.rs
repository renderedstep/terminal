//! Plain line mode, for scripts and agents: each line read from input is
//! played as a turn, and the turn's saved prose and outcome are written out.
//! No terminal control codes are ever written.

use std::io::{BufRead, Write};

use crate::engine::{follow_turn, new_request_token, Engine, Reconnect};
use crate::present;
use crate::protocol::{ErrorCode, FinishedEvent, Screen};

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Write each finished turn as one line of JSON instead of prose.
    pub json: bool,
}

enum Step {
    Follow(String),
    Send(String),
}

/// Returns how many lines the engine refused with an error.
pub fn run(
    engine: &dyn Engine,
    screen: &Screen,
    input: impl BufRead,
    out: &mut impl Write,
    options: Options,
    reconnect: &Reconnect,
) -> anyhow::Result<usize> {
    let game = screen.game.id.clone();
    if !options.json {
        if let Some(last) = screen.log.last() {
            writeln!(out, "{}\n", last.text)?;
        }
    }
    let mut glance = screen.glance.clone();
    let mut errors = 0;
    // A turn the engine is still playing is followed to its end before any
    // new line is read.
    let running = screen
        .standing
        .running_turn
        .clone()
        .filter(|_| screen.standing.busy)
        .map(|turn| Ok(Step::Follow(turn)));
    let typed = input.lines().map(|l| l.map(Step::Send));
    for step in running.into_iter().chain(typed) {
        let result = match step? {
            Step::Follow(turn) => follow_turn(engine, &game, &turn, reconnect, |_| {}),
            Step::Send(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let line = present::line_to_send(&glance, line);
                engine
                    .send_turn(&game, &line, &new_request_token())
                    .and_then(|turn| follow_turn(engine, &game, &turn.id, reconnect, |_| {}))
            }
        };
        match result {
            Ok(finished) => {
                write_finished(out, &finished, options)?;
                glance = finished.glance;
            }
            Err(e) => {
                errors += 1;
                let (message, hint) = present::error_lines(&e);
                if options.json {
                    let value = serde_json::json!({
                        "error": { "code": e.code(), "message": message }
                    });
                    writeln!(out, "{value}")?;
                } else {
                    writeln!(out, "! {message}")?;
                    if let Some(hint) = hint {
                        writeln!(out, "! {hint}")?;
                    }
                    writeln!(out)?;
                }
                out.flush()?;
                if e.code() == Some(ErrorCode::Unauthorized) {
                    break;
                }
            }
        }
    }
    Ok(errors)
}

fn write_finished(
    out: &mut impl Write,
    f: &FinishedEvent,
    options: Options,
) -> std::io::Result<()> {
    if options.json {
        writeln!(out, "{}", serde_json::to_string(f).expect("serializes"))?;
    } else {
        let text = present::finished_text(f);
        if !text.is_empty() {
            writeln!(out, "{text}")?;
        }
        for notice in &f.notices {
            writeln!(out, "* {notice}")?;
        }
        writeln!(out, "» {}\n", present::outcome_summary(f))?;
    }
    out.flush()
}
