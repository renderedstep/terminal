//! The start screen `ta` opens on: your games to continue, then the worlds a
//! new one can be started in, as the engine's web page lists them. Like
//! [`super::app`], nothing here touches the terminal or the network.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::protocol::{Game, World};

/// What the player picked.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// Open this game.
    Continue(String),
    /// Start a new game in this world.
    New(String),
}

/// The game `ta --resume` opens: the unfinished one started last. The
/// protocol gives no time a game was last played, so start time stands in.
pub fn latest_unfinished(games: &[Game]) -> Option<&Game> {
    games
        .iter()
        .filter(|g| !g.over)
        .max_by(|a, b| a.started_at.cmp(&b.started_at))
}

pub struct Start {
    pub engine_label: String,
    /// Unfinished games first, each group latest first.
    pub games: Vec<Game>,
    pub worlds: Vec<World>,
    /// Games and worlds counted as one list, games first.
    pub selected: usize,
    /// A note for the player, such as why a choice did not open.
    pub status: String,
    pub quit: bool,
}

impl Start {
    pub fn new(mut games: Vec<Game>, worlds: Vec<World>, engine_label: String) -> Self {
        games.sort_by(|a, b| {
            a.over
                .cmp(&b.over)
                .then_with(|| b.started_at.cmp(&a.started_at))
        });
        Self {
            engine_label,
            games,
            worlds,
            selected: 0,
            status: String::new(),
            quit: false,
        }
    }

    fn len(&self) -> usize {
        self.games.len() + self.worlds.len()
    }

    /// What Enter would pick now.
    pub fn choice(&self) -> Option<Choice> {
        match self.games.get(self.selected) {
            Some(g) => Some(Choice::Continue(g.id.clone())),
            None => self
                .worlds
                .get(self.selected - self.games.len())
                .map(|w| Choice::New(w.id.clone())),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<Choice> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let last = self.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d') if ctrl => self.quit = true,
            KeyCode::Esc | KeyCode::Char('q') => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::Enter => {
                let choice = self.choice();
                if choice.is_some() {
                    self.status = "Opening…".into();
                }
                return choice;
            }
            _ => {}
        }
        None
    }
}
