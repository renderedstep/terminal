//! The engine seam. The TUI and line mode talk only to [`Engine`], so the
//! HTTP implementation of protocol v1 can be replaced by an in-process engine
//! without touching them.

pub mod fake;
pub mod http;

use std::time::Duration;

use crate::protocol::{
    ErrorCode, FinishedEvent, Game, Screen, Service, Standing, Turn, TurnEvent, TurnEventKind,
    World,
};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    /// The engine answered with a protocol error. `message` is the engine's
    /// own sentence for the player and is shown as it is.
    #[error("{message}")]
    Api {
        status: u16,
        code: ErrorCode,
        message: String,
    },
    /// The engine could not be reached, or the connection dropped.
    #[error("could not reach the engine: {0}")]
    Transport(String),
    /// The engine answered with something protocol v1 does not describe.
    #[error("the engine sent something unexpected: {0}")]
    Protocol(String),
}

impl EngineError {
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Api { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Worth reconnecting a turn stream after.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transport(_))
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// A turn's events, in order. Ends (`None`) when the engine closes the stream.
pub type EventStream<'a> = Box<dyn Iterator<Item = Result<TurnEvent>> + Send + 'a>;

/// Everything the client needs from an engine. Mirrors protocol v1's
/// endpoints one to one.
pub trait Engine: Send + Sync {
    fn service(&self) -> Result<Service>;
    fn worlds(&self) -> Result<Vec<World>>;
    fn games(&self) -> Result<Vec<Game>>;
    fn start_game(&self, world: &str) -> Result<Screen>;
    fn screen(&self, game: &str) -> Result<Screen>;
    /// Send a typed line. The same `request_token` again is the same turn.
    fn send_turn(&self, game: &str, line: &str, request_token: &str) -> Result<Turn>;
    /// The turn's events after `last_event_id` (all of them when `None`).
    fn turn_events(
        &self,
        game: &str,
        turn: &str,
        last_event_id: Option<&str>,
    ) -> Result<EventStream<'_>>;
    fn acknowledge(&self, game: &str, turn: &str) -> Result<Standing>;
}

/// A fresh token for a new line, so a retry of the same send is the same turn.
pub fn new_request_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone)]
pub struct Reconnect {
    /// Attempts in a row that bring no new event before giving up.
    pub attempts: u32,
    pub backoff: Duration,
}

impl Default for Reconnect {
    fn default() -> Self {
        Self {
            attempts: 6,
            backoff: Duration::from_millis(500),
        }
    }
}

/// What [`follow_turn`] reports as it goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Event(TurnEvent),
    /// The stream dropped; reconnecting from `last_event_id`.
    Reconnecting {
        attempt: u32,
        last_event_id: Option<String>,
    },
}

/// Read a turn to its `finished` event, reconnecting with the last event id
/// whenever the stream drops or the engine closes it early. Events already
/// seen are never reported twice.
pub fn follow_turn(
    engine: &dyn Engine,
    game: &str,
    turn: &str,
    policy: &Reconnect,
    mut on: impl FnMut(Progress),
) -> Result<FinishedEvent> {
    let mut last: Option<String> = None;
    let mut failures = 0;
    loop {
        let mut progressed = false;
        let outcome = (|| -> Result<Option<FinishedEvent>> {
            let stream = engine.turn_events(game, turn, last.as_deref())?;
            for event in stream {
                let event = event?;
                progressed = true;
                last = Some(event.id.clone());
                let finished = match &event.kind {
                    TurnEventKind::Finished(f) => Some((**f).clone()),
                    _ => None,
                };
                on(Progress::Event(event));
                if finished.is_some() {
                    return Ok(finished);
                }
            }
            Ok(None)
        })();
        match outcome {
            Ok(Some(finished)) => return Ok(finished),
            Ok(None) => {}
            Err(e) if e.is_transient() => {
                if !progressed && failures + 1 >= policy.attempts {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }
        failures = if progressed { 0 } else { failures + 1 };
        if failures >= policy.attempts {
            return Err(EngineError::Transport(
                "the turn stream kept closing before the turn finished".into(),
            ));
        }
        on(Progress::Reconnecting {
            attempt: failures + 1,
            last_event_id: last.clone(),
        });
        if failures > 0 {
            std::thread::sleep(policy.backoff * failures);
        }
    }
}
