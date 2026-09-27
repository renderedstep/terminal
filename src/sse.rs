//! A minimal Server-Sent Events reader: enough of the WHATWG event-stream
//! format for the turn stream (`id`, `event`, `data`, comments, CRLF).

use std::io::BufRead;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawEvent {
    pub id: Option<String>,
    pub event: String,
    pub data: String,
}

/// Reads events one at a time from a byte stream.
pub struct SseReader<R> {
    inner: R,
}

impl<R: BufRead> SseReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// The next complete event, `Ok(None)` at a clean end of stream. A
    /// half-written event cut off by the end of the stream is dropped, as the
    /// format requires.
    pub fn next_event(&mut self) -> std::io::Result<Option<RawEvent>> {
        let mut current = RawEvent::default();
        let mut data_lines: Vec<String> = Vec::new();
        let mut seen_field = false;
        let mut line = String::new();
        loop {
            line.clear();
            if self.inner.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let trimmed = line.trim_end_matches(['\n', '\r']);
            if trimmed.is_empty() {
                if !seen_field || data_lines.is_empty() && current.event.is_empty() {
                    seen_field = false;
                    continue;
                }
                current.data = data_lines.join("\n");
                if current.event.is_empty() {
                    current.event = "message".into();
                }
                return Ok(Some(current));
            }
            if trimmed.starts_with(':') {
                continue;
            }
            let (field, value) = match trimmed.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (trimmed, ""),
            };
            seen_field = true;
            match field {
                "id" => current.id = Some(value.to_string()),
                "event" => current.event = value.to_string(),
                "data" => data_lines.push(value.to_string()),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(input: &str) -> Vec<RawEvent> {
        let mut r = SseReader::new(input.as_bytes());
        let mut out = vec![];
        while let Some(e) = r.next_event().unwrap() {
            out.push(e);
        }
        out
    }

    #[test]
    fn reads_events_comments_and_crlf() {
        let events = all(": hello\r\nid: 1\r\nevent: prose\r\ndata: {\"a\":1}\r\n\r\nid: 2\nevent: finished\ndata: x\ndata: y\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id.as_deref(), Some("1"));
        assert_eq!(events[0].event, "prose");
        assert_eq!(events[0].data, "{\"a\":1}");
        assert_eq!(events[1].data, "x\ny");
    }

    #[test]
    fn drops_an_event_cut_off_by_the_end() {
        let events = all("id: 1\nevent: prose\ndata: a\n\nid: 2\nevent: prose\ndata: b\n");
        assert_eq!(events.len(), 1);
    }
}
