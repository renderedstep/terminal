//! The examples published in the engine's contract (`spec/openapi.json`),
//! which the engine's own tests hold to its real responses. Tests here use
//! them as payloads, so the client and the engine test against the same data.

#![allow(dead_code)]

use serde_json::Value;

pub fn spec() -> Value {
    serde_json::from_str(include_str!("../../spec/openapi.json")).unwrap()
}

fn content(path: &str, method: &str, status: &str) -> Value {
    let spec = spec();
    let op = &spec["paths"][path][method];
    let body = if status == "request" {
        &op["requestBody"]
    } else {
        &op["responses"][status]
    };
    body["content"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

/// The example response body for one endpoint and status, with the name of
/// the schema it follows.
pub fn response(path: &str, method: &str, status: &str) -> (String, Value) {
    let c = content(path, method, status);
    let schema = c["schema"]["$ref"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    (schema, c["examples"]["example"]["value"].clone())
}

pub fn response_body(path: &str, method: &str, status: &str) -> Value {
    response(path, method, status).1
}

pub fn request_body(path: &str, method: &str) -> Value {
    content(path, method, "request")["examples"]["example"]["value"].clone()
}

/// The example turn stream, as the engine writes it on the wire.
pub fn event_stream() -> String {
    content("/api/v1/games/{game}/turns/{turn}/events", "get", "200")["examples"]["example"]
        ["value"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Every example response in the spec: (endpoint, status, schema, body).
pub fn all_responses() -> Vec<(String, String, String, Value)> {
    let spec = spec();
    let mut found = vec![];
    for (path, ops) in spec["paths"].as_object().unwrap() {
        for (method, op) in ops.as_object().unwrap() {
            for (status, r) in op["responses"].as_object().unwrap() {
                for c in r["content"]
                    .as_object()
                    .into_iter()
                    .flat_map(|c| c.values())
                {
                    let Some(schema) = c["schema"]["$ref"].as_str() else {
                        continue;
                    };
                    found.push((
                        format!("{} {path}", method.to_uppercase()),
                        status.clone(),
                        schema.rsplit('/').next().unwrap().to_string(),
                        c["examples"]["example"]["value"].clone(),
                    ));
                }
            }
        }
    }
    found
}
