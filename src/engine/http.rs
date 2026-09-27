//! Protocol v1 over HTTP and Server-Sent Events.

use std::io::BufReader;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION};
use reqwest::{Method, StatusCode, Url};
use serde::de::DeserializeOwned;

use super::{Engine, EngineError, EventStream, Result};
use crate::protocol::{
    ErrorBody, Game, GameList, NewGameRequest, Screen, Service, Standing, StandingResponse, Turn,
    TurnAccepted, TurnEvent, TurnEventKind, TurnRequest, World, WorldList,
};
use crate::sse::SseReader;

/// How long a turn stream may sit silent before it counts as dropped. The
/// engine closes idle streams itself well before this.
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(120);

pub struct HttpEngine {
    base: Url,
    token: String,
    client: Client,
}

impl std::fmt::Debug for HttpEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpEngine")
            .field("base", &self.base.as_str())
            .field("token", &"<hidden>")
            .finish()
    }
}

impl HttpEngine {
    /// `engine_url` is the engine's address, with or without `/api/v1`.
    pub fn new(engine_url: &str, token: &str) -> std::result::Result<Self, String> {
        let trimmed = engine_url.trim().trim_end_matches('/');
        let trimmed = trimmed.strip_suffix("/api/v1").unwrap_or(trimmed);
        let base = Url::parse(&format!("{trimmed}/api/v1"))
            .map_err(|e| format!("the engine address {engine_url:?} is not a URL: {e}"))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(format!(
                "the engine address {engine_url:?} must start with http:// or https://"
            ));
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!(
                "text-adventure-terminal/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            token: token.to_string(),
            client,
        })
    }

    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("http URLs have paths")
            .extend(segments);
        url
    }

    fn request(&self, method: Method, segments: &[&str]) -> RequestBuilder {
        self.client
            .request(method, self.url(segments))
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
    }

    fn send(&self, req: RequestBuilder) -> Result<Response> {
        let resp = req
            .send()
            .map_err(|e| EngineError::Transport(transport_message(e)))?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(error_from(resp))
        }
    }

    fn json<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let resp = self.send(req.header(ACCEPT, "application/json"))?;
        let body = resp
            .bytes()
            .map_err(|e| EngineError::Transport(transport_message(e)))?;
        serde_json::from_slice(&body).map_err(|e| EngineError::Protocol(e.to_string()))
    }
}

/// reqwest's messages can carry the full URL; keep only the cause.
fn transport_message(e: reqwest::Error) -> String {
    let e = e.without_url();
    let mut msg = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        msg = format!("{msg}: {s}");
        source = s.source();
    }
    msg
}

fn error_from(resp: Response) -> EngineError {
    let status = resp.status();
    let body = resp.bytes().unwrap_or_default();
    match serde_json::from_slice::<ErrorBody>(&body) {
        Ok(ErrorBody { error }) => EngineError::Api {
            status: status.as_u16(),
            code: error.code,
            message: error.message,
        },
        Err(_) => EngineError::Protocol(format!(
            "HTTP {} without a protocol error body",
            status.as_u16()
        )),
    }
}

impl Engine for HttpEngine {
    fn service(&self) -> Result<Service> {
        self.json(self.request(Method::GET, &[]))
    }

    fn worlds(&self) -> Result<Vec<World>> {
        self.json::<WorldList>(self.request(Method::GET, &["worlds"]))
            .map(|l| l.worlds)
    }

    fn games(&self) -> Result<Vec<Game>> {
        self.json::<GameList>(self.request(Method::GET, &["games"]))
            .map(|l| l.games)
    }

    fn start_game(&self, world: &str) -> Result<Screen> {
        self.json(
            self.request(Method::POST, &["games"])
                .json(&NewGameRequest {
                    world: world.to_string(),
                }),
        )
    }

    fn screen(&self, game: &str) -> Result<Screen> {
        self.json(self.request(Method::GET, &["games", game]))
    }

    fn send_turn(&self, game: &str, line: &str, request_token: &str) -> Result<Turn> {
        self.json::<TurnAccepted>(self.request(Method::POST, &["games", game, "turns"]).json(
            &TurnRequest {
                line: line.to_string(),
                request_token: Some(request_token.to_string()),
            },
        ))
        .map(|a| a.turn)
    }

    fn turn_events(
        &self,
        game: &str,
        turn: &str,
        last_event_id: Option<&str>,
    ) -> Result<EventStream<'_>> {
        let mut req = self
            .request(Method::GET, &["games", game, "turns", turn, "events"])
            .header(ACCEPT, "text/event-stream")
            .timeout(STREAM_READ_TIMEOUT);
        if let Some(id) = last_event_id {
            req = req.header("Last-Event-ID", id);
        }
        let resp = self.send(req)?;
        if resp.status() != StatusCode::OK {
            return Err(EngineError::Protocol(format!(
                "HTTP {} for a turn stream",
                resp.status().as_u16()
            )));
        }
        let mut reader = SseReader::new(BufReader::new(resp));
        Ok(Box::new(std::iter::from_fn(move || loop {
            let raw = match reader.next_event() {
                Ok(Some(raw)) => raw,
                Ok(None) => return None,
                Err(e) => return Some(Err(EngineError::Transport(e.to_string()))),
            };
            let Some(decoded) = TurnEventKind::decode(&raw.event, &raw.data) else {
                continue;
            };
            let Some(id) = raw.id else {
                return Some(Err(EngineError::Protocol(format!(
                    "a {} event without an id",
                    raw.event
                ))));
            };
            return Some(
                decoded
                    .map(|kind| TurnEvent { id, kind })
                    .map_err(|e| EngineError::Protocol(format!("{} event: {e}", raw.event))),
            );
        })))
    }

    fn acknowledge(&self, game: &str, turn: &str) -> Result<Standing> {
        self.json::<StandingResponse>(self.request(
            Method::POST,
            &["games", game, "interruptions", turn, "acknowledge"],
        ))
        .map(|r| r.standing)
    }
}
