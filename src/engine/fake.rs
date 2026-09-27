//! An in-memory engine for tests and demos. It plays every line the same
//! way (a short scene) unless a turn is scripted, and it can be told to fail
//! sends or drop streams part way so reconnecting can be exercised.

use std::collections::VecDeque;
use std::sync::Mutex;

use super::{Engine, EngineError, EventStream, Result};
use crate::protocol::*;

/// How one scripted turn plays out.
#[derive(Debug, Clone)]
pub struct ScriptedTurn {
    pub prose: Vec<String>,
    pub glance: Option<Glance>,
    pub finished: FinishedEvent,
}

#[derive(Default)]
struct State {
    screen: Option<Screen>,
    turns: Vec<(Turn, Vec<TurnEvent>)>,
    script: VecDeque<ScriptedTurn>,
    send_errors: VecDeque<EngineError>,
    /// Each connect to a stream yields at most this many events, then drops.
    drop_after: Option<usize>,
    drops_left: usize,
    connects: Vec<Option<String>>,
    sent_lines: Vec<(String, String)>,
}

pub struct FakeEngine {
    state: Mutex<State>,
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEngine {
    pub fn new() -> Self {
        let engine = Self {
            state: Mutex::new(State::default()),
        };
        engine.state.lock().unwrap().screen = Some(sample_screen());
        engine
    }

    pub fn script(&self, turn: ScriptedTurn) {
        self.state.lock().unwrap().script.push_back(turn);
    }

    pub fn fail_next_send(&self, error: EngineError) {
        self.state.lock().unwrap().send_errors.push_back(error);
    }

    /// The next `times` stream connections each end with a dropped
    /// connection after `after` events.
    pub fn drop_streams(&self, after: usize, times: usize) {
        let mut s = self.state.lock().unwrap();
        s.drop_after = Some(after);
        s.drops_left = times;
    }

    /// The `Last-Event-ID` each stream connection was opened with.
    pub fn connects(&self) -> Vec<Option<String>> {
        self.state.lock().unwrap().connects.clone()
    }

    /// Every line sent, with its request token.
    pub fn sent_lines(&self) -> Vec<(String, String)> {
        self.state.lock().unwrap().sent_lines.clone()
    }

    /// Send `line` as a turn and leave the game busy with it, as an engine
    /// does while the turn is still playing. Returns the turn's id.
    pub fn leave_running(&self, line: &str) -> String {
        let game = self
            .state
            .lock()
            .unwrap()
            .screen
            .as_ref()
            .unwrap()
            .game
            .id
            .clone();
        let turn = self.send_turn(&game, line, "running").unwrap();
        let mut s = self.state.lock().unwrap();
        let standing = &mut s.screen.as_mut().unwrap().standing;
        standing.busy = true;
        standing.running_turn = Some(turn.id.clone());
        turn.id
    }

    pub fn set_screen(&self, screen: Screen) {
        self.state.lock().unwrap().screen = Some(screen);
    }

    fn play(&self, s: &mut State, line: &str, turn_id: &str) -> Vec<TurnEvent> {
        let glance = s.screen.as_ref().map(|sc| sc.glance.clone()).unwrap();
        let scripted = s.script.pop_front().unwrap_or_else(|| ScriptedTurn {
            prose: vec![
                format!("You try to {line}. "),
                "The fog does not care either way.".into(),
            ],
            glance: None,
            finished: FinishedEvent {
                turn: String::new(),
                outcome: Outcome {
                    kind: OutcomeKind::Narrated,
                },
                text: Some(format!(
                    "You try to {line}. The fog does not care either way."
                )),
                notices: vec![],
                glance: glance.clone(),
                standing: sample_standing(),
                refusal: None,
                resolved_by: Some("fake".into()),
                rolls: vec![],
            },
        });
        let mut events = vec![TurnEventKind::Started(StartedEvent {
            turn: turn_id.into(),
            line: line.into(),
        })];
        if let Some(g) = scripted.glance {
            events.push(TurnEventKind::Glance(Box::new(GlanceEvent {
                turn: turn_id.into(),
                glance: g,
            })));
        }
        for text in scripted.prose {
            events.push(TurnEventKind::Prose(ProseEvent {
                turn: turn_id.into(),
                text,
            }));
        }
        let mut finished = scripted.finished;
        finished.turn = turn_id.into();
        if let Some(sc) = s.screen.as_mut() {
            sc.glance = finished.glance.clone();
            sc.standing = finished.standing.clone();
            sc.log.push(LogEntry {
                id: turn_id.into(),
                typed: Some(line.into()),
                text: finished.text.clone().unwrap_or_default(),
                talking_to: vec![],
                notices: finished.notices.clone(),
            });
        }
        events.push(TurnEventKind::Finished(Box::new(finished)));
        events
            .into_iter()
            .enumerate()
            .map(|(i, kind)| TurnEvent {
                id: (i + 1).to_string(),
                kind,
            })
            .collect()
    }
}

fn not_found() -> EngineError {
    EngineError::Api {
        status: 404,
        code: ErrorCode::NotFound,
        message: "There is no such game.".into(),
    }
}

impl Engine for FakeEngine {
    fn service(&self) -> Result<Service> {
        Ok(sample_service())
    }

    fn worlds(&self) -> Result<Vec<World>> {
        Ok(vec![sample_world()])
    }

    fn games(&self) -> Result<Vec<Game>> {
        let s = self.state.lock().unwrap();
        Ok(s.screen.iter().map(|sc| sc.game.clone()).collect())
    }

    fn start_game(&self, world: &str) -> Result<Screen> {
        if world != sample_world().id {
            return Err(EngineError::Api {
                status: 422,
                code: ErrorCode::Unplayable,
                message: "That world has no starting place.".into(),
            });
        }
        let screen = sample_screen();
        self.state.lock().unwrap().screen = Some(screen.clone());
        Ok(screen)
    }

    fn screen(&self, game: &str) -> Result<Screen> {
        let s = self.state.lock().unwrap();
        s.screen
            .clone()
            .filter(|sc| sc.game.id == game)
            .ok_or_else(not_found)
    }

    fn send_turn(&self, game: &str, line: &str, request_token: &str) -> Result<Turn> {
        let mut s = self.state.lock().unwrap();
        if s.screen.as_ref().map(|sc| sc.game.id.as_str()) != Some(game) {
            return Err(not_found());
        }
        if let Some(e) = s.send_errors.pop_front() {
            return Err(e);
        }
        if line.trim().is_empty() {
            return Err(EngineError::Api {
                status: 422,
                code: ErrorCode::BlankLine,
                message: "Type something to do.".into(),
            });
        }
        s.sent_lines.push((line.into(), request_token.into()));
        if let Some((turn, _)) = s
            .turns
            .iter()
            .find(|(t, _)| t.request_token == request_token)
        {
            return Ok(turn.clone());
        }
        let turn = Turn {
            id: format!("turn-{}", s.turns.len() + 1),
            line: line.into(),
            request_token: request_token.into(),
        };
        let events = self.play(&mut s, line, &turn.id);
        s.turns.push((turn.clone(), events));
        Ok(turn)
    }

    fn turn_events(
        &self,
        game: &str,
        turn: &str,
        last_event_id: Option<&str>,
    ) -> Result<EventStream<'_>> {
        let mut s = self.state.lock().unwrap();
        if s.screen.as_ref().map(|sc| sc.game.id.as_str()) != Some(game) {
            return Err(not_found());
        }
        s.connects.push(last_event_id.map(str::to_string));
        let events = s
            .turns
            .iter()
            .find(|(t, _)| t.id == turn)
            .map(|(_, e)| e.clone())
            .ok_or_else(not_found)?;
        let after: usize = last_event_id.and_then(|id| id.parse().ok()).unwrap_or(0);
        let mut items: Vec<Result<TurnEvent>> = events
            .into_iter()
            .filter(|e| e.id.parse::<usize>().unwrap_or(0) > after)
            .map(Ok)
            .collect();
        if s.drops_left > 0 {
            if let Some(n) = s.drop_after {
                if items.len() > n {
                    s.drops_left -= 1;
                    items.truncate(n);
                    items.push(Err(EngineError::Transport("connection reset".into())));
                }
            }
        }
        Ok(Box::new(items.into_iter()))
    }

    fn acknowledge(&self, game: &str, _turn: &str) -> Result<Standing> {
        let mut s = self.state.lock().unwrap();
        let sc = s
            .screen
            .as_mut()
            .filter(|sc| sc.game.id == game)
            .ok_or_else(not_found)?;
        sc.standing.saved_turn = None;
        Ok(sc.standing.clone())
    }
}

pub fn sample_world() -> World {
    World {
        id: "world-fog".into(),
        title: "The Fog Harbour".into(),
        summary: "A harbour town where the fog keeps the hours.".into(),
    }
}

pub fn sample_service() -> Service {
    Service {
        protocol: PROTOCOL.into(),
        version: VERSION,
        capabilities: [
            "worlds",
            "games",
            "turns",
            "turn_events",
            "interruptions",
            "spend_limit",
        ]
        .map(String::from)
        .to_vec(),
        player: Player {
            name: "Wren".into(),
            spend: Spend {
                used_usd: 0.42,
                held_usd: 0.0,
                limit_usd: 5.0,
                turn_reservation_usd: 0.05,
                period_ends_at: "2026-10-01T00:00:00Z".into(),
            },
        },
        engine: EngineIdentity {
            name: "fake".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        world_schema: "1".into(),
    }
}

pub fn sample_standing() -> Standing {
    Standing {
        over: false,
        ended: None,
        busy: false,
        running_turn: None,
        saved_turn: None,
    }
}

fn verb(name: VerbName, word: &str, targets: &[&str]) -> Verb {
    Verb {
        name,
        available: !targets.is_empty(),
        reason: if targets.is_empty() {
            Some("There is nothing here for that.".into())
        } else {
            None
        },
        targets: targets.iter().map(|t| t.to_string()).collect(),
        aims: None,
        word: Some(word.into()),
        lines: None,
    }
}

fn use_verb(targets: &[(&str, &str)]) -> Verb {
    Verb {
        name: VerbName::Use,
        available: !targets.is_empty(),
        reason: None,
        targets: targets.iter().map(|(t, _)| t.to_string()).collect(),
        aims: None,
        word: None,
        lines: Some(targets.iter().map(|(_, l)| Some(l.to_string())).collect()),
    }
}

pub fn sample_glance() -> Glance {
    Glance {
        room: Some(Room {
            name: "The Tide Post".into(),
            within: Some("Gallows Rock".into()),
        }),
        exits: vec![
            Exit {
                name: "The Causeway Court".into(),
                written: true,
                open: true,
            },
            Exit {
                name: "The Sea Stair".into(),
                written: false,
                open: true,
            },
        ],
        people: vec![
            Person {
                name: "Neb Halloran".into(),
                condition: None,
                foe: false,
                provoked: false,
            },
            Person {
                name: "The Tollman".into(),
                condition: Some("wounded".into()),
                foe: true,
                provoked: true,
            },
        ],
        lying_here: vec![Thing {
            name: "Filing press".into(),
        }],
        carrying: vec![Thing {
            name: "Assize tide-slate".into(),
        }],
        condition: Some("hurt (14 of 18)".into()),
        sheet: vec!["HP 14/18 · AC 12".into()],
        next_beat: Some("Find out who wrote on the slate.".into()),
        story_time: Some("2026-03-01T20:00:00Z".into()),
        over: false,
        ended: None,
        verbs: vec![
            verb(
                VerbName::Move,
                "go",
                &["The Causeway Court", "The Sea Stair"],
            ),
            verb(VerbName::Talk, "talk", &["Neb Halloran", "The Tollman"]),
            verb(
                VerbName::Examine,
                "inspect",
                &["Filing press", "Assize tide-slate", "Neb Halloran"],
            ),
            verb(VerbName::Take, "take", &["Filing press"]),
            verb(VerbName::Drop, "drop", &["Assize tide-slate"]),
            verb(VerbName::Attack, "attack", &["The Tollman"]),
            verb(VerbName::Throw, "throw", &["Assize tide-slate"]),
            use_verb(&[(
                "Offer Assize tide-slate to Neb Halloran; they may refuse",
                "/offer Assize tide-slate to Neb Halloran",
            )]),
        ],
    }
}

pub fn sample_screen() -> Screen {
    Screen {
        game: Game {
            id: "game-1".into(),
            world: sample_world(),
            over: false,
            started_at: "2026-09-20T18:00:00Z".into(),
        },
        glance: sample_glance(),
        standing: sample_standing(),
        log: vec![LogEntry {
            id: "turn-0".into(),
            typed: None,
            text: "The chalk is still under your thumb and it is still not your handwriting.\n\nJusticar Brace waits on the bench.".into(),
            talking_to: vec![],
            notices: vec![],
        }],
    }
}
