//! The engine protocol, version 1, as Rust types.
//!
//! Each struct mirrors one schema in `spec/openapi.json` (a copy of the
//! engine's published contract). `tests/contract.rs` validates example
//! payloads against that file and checks that every one round-trips through
//! these types unchanged, so a field the spec has and these types lack fails
//! the build.
//!
//! This is revision 1.1. v1 only grows: unknown fields are ignored on read. Enums are closed in the
//! spec, but a value outside the list reads as `Unknown` rather than failing
//! the whole payload, so one engine bug cannot blank the screen.

use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "text-adventure-engine";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    NotFound,
    LimitReached,
    RateLimited,
    BlankLine,
    Conflict,
    Unplayable,
    Invalid,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Service {
    pub protocol: String,
    pub version: u32,
    pub capabilities: Vec<String>,
    pub player: Player,
    pub engine: EngineIdentity,
    pub world_schema: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    pub spend: Spend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spend {
    pub used_usd: f64,
    pub held_usd: f64,
    pub limit_usd: f64,
    pub turn_reservation_usd: f64,
    pub period_ends_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldList {
    pub worlds: Vec<World>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub world: World,
    pub over: bool,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameList {
    pub games: Vec<Game>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exit {
    pub name: String,
    pub written: bool,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub condition: Option<String>,
    pub foe: bool,
    pub provoked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thing {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerbName {
    Move,
    Talk,
    Examine,
    Take,
    Drop,
    Attack,
    Throw,
    Use,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verb {
    pub name: VerbName,
    pub available: bool,
    pub reason: Option<String>,
    pub targets: Vec<String>,
    pub aims: Option<Vec<String>>,
    /// What the engine reads after a slash for this verb (`/word target`).
    /// `None` for `use`, and from an engine older than revision 1.1.
    pub word: Option<String>,
    /// For `use` only: the exact line that plays each target, in the order of
    /// `targets`. An entry is `None` when no line plays that target alone.
    pub lines: Option<Vec<Option<String>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub within: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Glance {
    pub room: Option<Room>,
    pub exits: Vec<Exit>,
    pub people: Vec<Person>,
    pub lying_here: Vec<Thing>,
    pub carrying: Vec<Thing>,
    pub condition: Option<String>,
    pub sheet: Vec<String>,
    pub next_beat: Option<String>,
    pub story_time: Option<String>,
    pub over: bool,
    pub ended: Option<String>,
    pub verbs: Vec<Verb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedAction {
    Resume,
    Acknowledge,
    None,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedTurn {
    pub turn: String,
    pub line: String,
    pub action: SavedAction,
    pub request_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Standing {
    pub over: bool,
    pub ended: Option<String>,
    pub busy: bool,
    /// While `busy`, the turn in hand, whose events can be followed.
    pub running_turn: Option<String>,
    pub saved_turn: Option<SavedTurn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub typed: Option<String>,
    pub text: String,
    pub talking_to: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    pub game: Game,
    pub glance: Glance,
    pub standing: Standing,
    pub log: Vec<LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub line: String,
    pub request_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnAccepted {
    pub turn: Turn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandingResponse {
    pub standing: Standing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGameRequest {
    pub world: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartedEvent {
    pub turn: String,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseEvent {
    pub turn: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlanceEvent {
    pub turn: String,
    pub glance: Glance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Moved,
    Talked,
    Took,
    Dropped,
    Read,
    Attacked,
    Threw,
    Used,
    Ended,
    Narrated,
    Refused,
    Safety,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub kind: OutcomeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    NamedMoreThanOne,
    Unresolved,
    Immovable,
    Unreadable,
    Unplayable,
    Dead,
    Concluded,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub kind: RefusalKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollKind {
    Check,
    Blow,
    Toll,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Roll {
    pub kind: RollKind,
    pub die: Option<i64>,
    pub result: Option<i64>,
    pub target: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinishedEvent {
    pub turn: String,
    pub outcome: Outcome,
    pub text: Option<String>,
    pub notices: Vec<String>,
    pub glance: Glance,
    pub standing: Standing,
    pub refusal: Option<Refusal>,
    pub resolved_by: Option<String>,
    pub rolls: Vec<Roll>,
}

/// One event of a turn's stream, with its SSE id.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnEvent {
    pub id: String,
    pub kind: TurnEventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnEventKind {
    Started(StartedEvent),
    Prose(ProseEvent),
    Glance(Box<GlanceEvent>),
    Finished(Box<FinishedEvent>),
}

impl TurnEventKind {
    /// Decode one event from its SSE `event` name and `data` line. An event
    /// name this version does not know yields `None` and is skipped.
    pub fn decode(event: &str, data: &str) -> Option<Result<Self, serde_json::Error>> {
        Some(match event {
            "started" => serde_json::from_str(data).map(Self::Started),
            "prose" => serde_json::from_str(data).map(Self::Prose),
            "glance" => serde_json::from_str(data).map(|g| Self::Glance(Box::new(g))),
            "finished" => serde_json::from_str(data).map(|f| Self::Finished(Box::new(f))),
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Started(_) => "started",
            Self::Prose(_) => "prose",
            Self::Glance(_) => "glance",
            Self::Finished(_) => "finished",
        }
    }

    pub fn data_json(&self) -> String {
        match self {
            Self::Started(e) => serde_json::to_string(e),
            Self::Prose(e) => serde_json::to_string(e),
            Self::Glance(e) => serde_json::to_string(e),
            Self::Finished(e) => serde_json::to_string(e),
        }
        .expect("protocol types serialize")
    }
}

impl Glance {
    pub fn verb(&self, name: VerbName) -> Option<&Verb> {
        self.verbs.iter().find(|v| v.name == name)
    }
}
