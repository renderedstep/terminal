//! The HTTP engine against a scripted local server: requests, errors in the
//! engine's words, and resuming a dropped turn stream with Last-Event-ID.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use text_adventure_terminal::engine::http::HttpEngine;
use text_adventure_terminal::engine::{follow_turn, Engine, EngineError, Progress, Reconnect};
use text_adventure_terminal::protocol::{ErrorCode, TurnEventKind};

#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// What the server does with one request.
enum Reply {
    Json(u16, String),
    /// An event stream, closed after `body` (mid-event, if `body` ends
    /// there, as a dropped connection would).
    Events {
        body: String,
    },
}

type Handler = Box<dyn Fn(&Request, usize) -> Reply + Send + Sync>;

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into(),
    })
}

fn serve(handler: Handler) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(vec![]));
    let log = requests.clone();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { continue };
            let Some(req) = read_request(&mut stream) else {
                continue;
            };
            log.lock().unwrap().push(req.clone());
            let reply = handler(&req, n);
            let response = match reply {
                Reply::Json(status, body) => format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                Reply::Events { body } => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
                ),
            };
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    Server { url, requests }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn compact(json: &str) -> String {
    serde_json::to_string(&serde_json::from_str::<serde_json::Value>(json).unwrap()).unwrap()
}

#[test]
fn sends_the_bearer_token_and_reads_the_service() {
    let server = serve(Box::new(|_, _| Reply::Json(200, fixture("Service"))));
    let engine = HttpEngine::new(&format!("{}/", server.url), "secret-token").unwrap();
    let service = engine.service().unwrap();
    assert_eq!(service.engine.name, "example-engine");
    let reqs = server.requests.lock().unwrap();
    assert_eq!(reqs[0].path, "/api/v1");
    assert_eq!(reqs[0].header("authorization"), Some("Bearer secret-token"));
}

#[test]
fn accepts_an_address_that_already_ends_in_the_api_path() {
    let server = serve(Box::new(|_, _| Reply::Json(200, fixture("WorldList"))));
    let engine = HttpEngine::new(&format!("{}/api/v1", server.url), "t").unwrap();
    engine.worlds().unwrap();
    assert_eq!(server.requests.lock().unwrap()[0].path, "/api/v1/worlds");
}

#[test]
fn ids_are_sent_back_opaquely() {
    let server = serve(Box::new(|_, _| Reply::Json(202, fixture("TurnAccepted"))));
    let engine = HttpEngine::new(&server.url, "t").unwrap();
    let turn = engine
        .send_turn("g/1 x", "/take Filing press", "rt-1")
        .unwrap();
    assert_eq!(turn.id, "t3");
    let reqs = server.requests.lock().unwrap();
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/api/v1/games/g%2F1%20x/turns");
    let body: serde_json::Value = serde_json::from_str(&reqs[0].body).unwrap();
    assert_eq!(body["line"], "/take Filing press");
    assert_eq!(body["request_token"], "rt-1");
}

#[test]
fn errors_carry_the_engines_own_words() {
    for (status, fixture_name, code) in [
        (402, "Error.limit", ErrorCode::LimitReached),
        (429, "Error.rate", ErrorCode::RateLimited),
    ] {
        let body = fixture(fixture_name);
        let server = serve(Box::new(move |_, _| Reply::Json(status, body.clone())));
        let engine = HttpEngine::new(&server.url, "t").unwrap();
        let err = engine.send_turn("g1", "look", "rt").unwrap_err();
        let expected: serde_json::Value = serde_json::from_str(&fixture(fixture_name)).unwrap();
        assert_eq!(err.code(), Some(code));
        assert_eq!(
            err.to_string(),
            expected["error"]["message"].as_str().unwrap()
        );
    }
    let server = serve(Box::new(|_, _| {
        Reply::Json(
            401,
            r#"{"error":{"code":"unauthorized","message":"That token is not recognised."}}"#.into(),
        )
    }));
    let err = HttpEngine::new(&server.url, "bad")
        .unwrap()
        .games()
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::Api {
            status: 401,
            code: ErrorCode::Unauthorized,
            ..
        }
    ));
    assert_eq!(err.to_string(), "That token is not recognised.");
}

#[test]
fn a_refused_connection_is_a_transport_error_without_the_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let err = HttpEngine::new(&url, "secret-token")
        .unwrap()
        .service()
        .unwrap_err();
    assert!(err.is_transient());
    assert!(!err.to_string().contains("secret-token"));
}

fn event(id: u32, name: &str, data: &str) -> String {
    format!("id: {id}\nevent: {name}\ndata: {}\n\n", compact(data))
}

#[test]
fn a_dropped_stream_resumes_from_the_last_event_id() {
    let events = [
        event(1, "started", &fixture("StartedEvent")),
        event(2, "prose", &fixture("ProseEvent")),
        event(
            3,
            "prose",
            r#"{"turn":"t3","text":"The Tollman staggers."}"#,
        ),
        event(4, "finished", &fixture("FinishedEvent")),
    ];
    let server = serve(Box::new(move |req, _| {
        let after: usize = req
            .header("last-event-id")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let rest: Vec<String> = events.iter().skip(after).cloned().collect();
        if after == 0 {
            // Two whole events, then half of the third, then the line drops.
            let cut = &rest[2][..rest[2].len() / 2];
            Reply::Events {
                body: format!(": ok\n\n{}{}{cut}", rest[0], rest[1]),
            }
        } else {
            Reply::Events {
                body: rest.concat(),
            }
        }
    }));
    let engine = HttpEngine::new(&server.url, "t").unwrap();
    let mut seen = vec![];
    let policy = Reconnect {
        attempts: 3,
        backoff: Duration::ZERO,
    };
    let finished = follow_turn(&engine, "g1", "t3", &policy, |p| match p {
        Progress::Event(e) => seen.push((e.id, e.kind.name())),
        Progress::Reconnecting { last_event_id, .. } => {
            seen.push((format!("reconnect after {last_event_id:?}"), "-"))
        }
    })
    .unwrap();
    assert_eq!(finished.rolls.len(), 3);
    assert_eq!(
        seen,
        vec![
            ("1".to_string(), "started"),
            ("2".to_string(), "prose"),
            ("reconnect after Some(\"2\")".to_string(), "-"),
            ("3".to_string(), "prose"),
            ("4".to_string(), "finished"),
        ]
    );
    let reqs = server.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].path, "/api/v1/games/g1/turns/t3/events");
    assert_eq!(reqs[0].header("last-event-id"), None);
    assert_eq!(reqs[1].header("last-event-id"), Some("2"));
    assert!(matches!(
        text_adventure_terminal::protocol::TurnEventKind::decode(
            "glance",
            &compact(&fixture("GlanceEvent"))
        ),
        Some(Ok(TurnEventKind::Glance(_)))
    ));
}

#[test]
fn a_stream_that_never_finishes_gives_up() {
    let server = serve(Box::new(|_, _| Reply::Events {
        body: String::new(),
    }));
    let engine = HttpEngine::new(&server.url, "t").unwrap();
    let policy = Reconnect {
        attempts: 3,
        backoff: Duration::ZERO,
    };
    let err = follow_turn(&engine, "g1", "t3", &policy, |_| {}).unwrap_err();
    assert!(err.is_transient());
    assert_eq!(server.requests.lock().unwrap().len(), 3);
}
