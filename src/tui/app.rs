//! The TUI's state and how it changes. Nothing here touches the terminal or
//! the network: keys and engine replies go in, commands for the engine come
//! out, so every screen state can be built in a test.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::engine::{new_request_token, EngineError, Progress};
use crate::present;
use crate::protocol::{
    FinishedEvent, Glance, OutcomeKind, SavedAction, Screen, Standing, Turn, TurnEventKind,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A line the player typed.
    Typed(String),
    /// Saved prose from the engine.
    Prose(String),
    /// A refusal, in the engine's words.
    Refusal(String),
    /// The outcome and dice under a turn.
    Outcome(String),
    /// The engine's notices, and the client's own notes.
    Notice(String),
    /// An error, in the engine's words where it sent any.
    Error(String),
}

/// What the TUI asks the engine to do; the run loop carries it out.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Send { line: String, request_token: String },
    Follow { turn: String },
    Acknowledge { turn: String },
    Refresh,
}

/// Replies from the engine, fed back in by the run loop.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineMsg {
    Accepted(Turn),
    Progress(Progress),
    Finished(Box<FinishedEvent>),
    Failed(EngineError),
    Screen(Box<Screen>),
    Standing(Standing),
}

pub struct App {
    pub title: String,
    pub engine_label: String,
    pub glance: Glance,
    pub standing: Standing,
    pub log: Vec<Entry>,
    /// Prose streaming in for the running turn; replaced by the saved text
    /// when the turn finishes.
    pub preview: String,
    pub input: String,
    pub completion: Option<(Vec<String>, usize)>,
    pub busy: bool,
    pub status: String,
    /// Lines scrolled up from the bottom of the log.
    pub scroll: u16,
    pub quit: bool,
}

impl App {
    pub fn new(screen: Screen, engine_label: String) -> Self {
        let mut app = Self {
            title: screen.game.world.title.clone(),
            engine_label,
            glance: screen.glance.clone(),
            standing: screen.standing.clone(),
            log: vec![],
            preview: String::new(),
            input: String::new(),
            completion: None,
            busy: false,
            status: String::new(),
            scroll: 0,
            quit: false,
        };
        app.load_log(&screen);
        app
    }

    /// What to do on opening the game: follow the turn the engine is still
    /// playing, if any, so its prose streams in as it would have.
    pub fn start(&mut self) -> Option<Command> {
        self.follow_running()
    }

    fn follow_running(&mut self) -> Option<Command> {
        if self.busy || !self.standing.busy {
            return None;
        }
        let turn = self.standing.running_turn.clone()?;
        self.busy = true;
        self.status = "A turn is still playing; following it…".into();
        Some(Command::Follow { turn })
    }

    fn load_log(&mut self, screen: &Screen) {
        self.log.clear();
        for entry in &screen.log {
            if let Some(typed) = &entry.typed {
                self.log.push(Entry::Typed(typed.clone()));
            }
            if !entry.text.is_empty() {
                self.log.push(Entry::Prose(entry.text.clone()));
            }
            self.log
                .extend(entry.notices.iter().cloned().map(Entry::Notice));
        }
        self.note_standing();
    }

    fn note_standing(&mut self) {
        if let Some(saved) = &self.standing.saved_turn {
            let note = match saved.action {
                SavedAction::Resume => format!(
                    "A turn was interrupted: \"{}\". Type :resume to finish it.",
                    saved.line
                ),
                SavedAction::Acknowledge => format!(
                    "A turn could not finish: \"{}\". Type :ack to let it go (its effects stay).",
                    saved.line
                ),
                _ => return,
            };
            self.log.push(Entry::Notice(note));
        }
        if self.standing.over {
            let why = self.standing.ended.clone().unwrap_or_default();
            self.log.push(Entry::Notice(
                format!("The game is over. {why}").trim().into(),
            ));
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Command> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code != KeyCode::Tab {
            self.completion = None;
        }
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d') if ctrl => self.quit = true,
            KeyCode::Char('u') if ctrl => self.input.clear(),
            KeyCode::Esc => self.quit = true,
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Tab => self.complete(),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(5),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(5),
            KeyCode::Enter => return self.submit(),
            _ => {}
        }
        None
    }

    fn complete(&mut self) {
        match &mut self.completion {
            Some((choices, i)) if !choices.is_empty() => {
                *i = (*i + 1) % choices.len();
                self.input = choices[*i].clone();
            }
            _ => {
                let choices = present::completions(&self.glance, &self.input);
                if let Some(first) = choices.first() {
                    self.input = first.clone();
                }
                self.completion = Some((choices, 0));
            }
        }
    }

    fn submit(&mut self) -> Option<Command> {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return None;
        }
        match line.as_str() {
            ":q" | ":quit" => {
                self.quit = true;
                return None;
            }
            ":refresh" => {
                self.input.clear();
                return Some(Command::Refresh);
            }
            ":help" => {
                self.input.clear();
                self.scroll = 0;
                self.log.push(Entry::Notice(present::TUI_HELP.into()));
                return None;
            }
            ":resume" | ":ack" => {
                self.input.clear();
                return self.handle_saved(&line);
            }
            _ => {}
        }
        if let Some(text) = present::panel_listing(&self.glance, &line) {
            self.input.clear();
            self.scroll = 0;
            self.log.push(Entry::Notice(text));
            return None;
        }
        if self.busy {
            self.status = "A turn is still running.".into();
            return None;
        }
        let line = present::line_to_send(&self.glance, &line);
        self.input.clear();
        self.scroll = 0;
        self.busy = true;
        self.status = "Sending…".into();
        self.log.push(Entry::Typed(line.clone()));
        Some(Command::Send {
            line,
            request_token: new_request_token(),
        })
    }

    fn handle_saved(&mut self, cmd: &str) -> Option<Command> {
        let Some(saved) = self.standing.saved_turn.clone() else {
            self.status = "No turn is waiting.".into();
            return None;
        };
        match (cmd, saved.action) {
            (":resume", SavedAction::Resume) if !self.busy => {
                self.busy = true;
                self.status = "Resuming…".into();
                self.log.push(Entry::Typed(saved.line.clone()));
                Some(Command::Send {
                    line: saved.line,
                    request_token: saved.request_token,
                })
            }
            (":ack", SavedAction::Acknowledge) => Some(Command::Acknowledge { turn: saved.turn }),
            _ => {
                self.status = "That turn cannot be handled that way.".into();
                None
            }
        }
    }

    pub fn on_engine(&mut self, msg: EngineMsg) -> Option<Command> {
        match msg {
            EngineMsg::Accepted(turn) => {
                self.status = "Playing…".into();
                return Some(Command::Follow { turn: turn.id });
            }
            EngineMsg::Progress(Progress::Event(event)) => match event.kind {
                TurnEventKind::Started(_) => {}
                TurnEventKind::Prose(p) => self.preview.push_str(&p.text),
                TurnEventKind::Glance(g) => self.glance = g.glance,
                // Handled as `Finished`, which the run loop sends after it.
                TurnEventKind::Finished(_) => {}
            },
            EngineMsg::Progress(Progress::Reconnecting { attempt, .. }) => {
                self.status = format!("Connection dropped; reconnecting (try {attempt})…");
            }
            EngineMsg::Finished(f) => self.finish(*f),
            EngineMsg::Failed(e) => {
                self.busy = false;
                self.preview.clear();
                let (message, hint) = present::error_lines(&e);
                self.log.push(Entry::Error(message));
                if let Some(hint) = hint {
                    self.log.push(Entry::Notice(hint.into()));
                }
                self.status.clear();
            }
            EngineMsg::Screen(screen) => {
                self.title = screen.game.world.title.clone();
                self.glance = screen.glance.clone();
                self.standing = screen.standing.clone();
                self.load_log(&screen);
                self.status = "Redrawn from the engine.".into();
                return self.follow_running();
            }
            EngineMsg::Standing(standing) => {
                self.standing = standing;
                self.status = "Let the interrupted turn go.".into();
            }
        }
        None
    }

    fn finish(&mut self, f: FinishedEvent) {
        self.busy = false;
        self.preview.clear();
        let text = present::finished_text(&f);
        if !text.is_empty() {
            self.log.push(if f.outcome.kind == OutcomeKind::Refused {
                Entry::Refusal(text)
            } else {
                Entry::Prose(text)
            });
        }
        self.log
            .extend(f.notices.iter().cloned().map(Entry::Notice));
        self.log.push(Entry::Outcome(present::outcome_summary(&f)));
        self.glance = f.glance;
        self.standing = f.standing;
        self.status.clear();
        self.note_standing();
    }
}
