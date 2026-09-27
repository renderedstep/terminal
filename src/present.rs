//! Words the client shows, shared by the TUI and line mode.

use crate::engine::EngineError;
use crate::protocol::{ErrorCode, FinishedEvent, Glance, OutcomeKind, Roll, RollKind, VerbName};

/// The word a player types for a verb. Protocol v1 names verbs but not the
/// words the engine reads for them, so this table follows the engine's typed
/// grammar: `/go` moves and `/inspect` examines.
pub fn typed_word(verb: VerbName) -> Option<&'static str> {
    Some(match verb {
        VerbName::Move => "go",
        VerbName::Talk => "talk",
        VerbName::Examine => "inspect",
        VerbName::Take => "take",
        VerbName::Drop => "drop",
        VerbName::Attack => "attack",
        VerbName::Throw => "throw",
        VerbName::Use => "use",
        VerbName::Unknown => return None,
    })
}

pub fn outcome_word(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::Moved => "moved",
        OutcomeKind::Talked => "talked",
        OutcomeKind::Took => "took",
        OutcomeKind::Dropped => "dropped",
        OutcomeKind::Read => "read",
        OutcomeKind::Attacked => "attacked",
        OutcomeKind::Threw => "threw",
        OutcomeKind::Used => "used",
        OutcomeKind::Ended => "the story ended",
        OutcomeKind::Narrated => "narrated",
        OutcomeKind::Refused => "refused",
        OutcomeKind::Safety => "stepped out of the story",
        OutcomeKind::Failed => "failed",
        OutcomeKind::Unknown => "unknown outcome",
    }
}

pub fn roll_line(roll: &Roll) -> String {
    let n = |v: Option<i64>| v.map_or("?".to_string(), |v| v.to_string());
    match roll.kind {
        RollKind::Check => {
            let die = roll.die.map_or("d20".to_string(), |d| format!("d{d}"));
            let verdict = match (roll.result, roll.target) {
                (Some(r), Some(t)) if r >= t => " (made it)",
                (Some(_), Some(_)) => " (missed)",
                _ => "",
            };
            format!(
                "check {die}: {} against {}{verdict}",
                n(roll.result),
                n(roll.target)
            )
        }
        RollKind::Blow => format!("blow: {} damage", n(roll.result)),
        RollKind::Toll => format!("toll: {} damage", n(roll.result)),
        RollKind::Unknown => format!("roll: {}", n(roll.result)),
    }
}

/// The one-line summary under a finished turn: outcome, then rolls.
pub fn outcome_summary(f: &FinishedEvent) -> String {
    let mut parts = vec![outcome_word(f.outcome.kind).to_string()];
    parts.extend(f.rolls.iter().map(roll_line));
    parts.join(" · ")
}

/// The text a finished turn saved: the engine's record, which replaces the
/// streamed preview.
pub fn finished_text(f: &FinishedEvent) -> String {
    f.text
        .clone()
        .or_else(|| f.refusal.as_ref().map(|r| r.text.clone()))
        .unwrap_or_default()
}

/// An error as the player sees it: the engine's own sentence when it sent
/// one, with a short note on what to do where the client can help.
pub fn error_lines(e: &EngineError) -> (String, Option<&'static str>) {
    let hint = match e.code() {
        Some(ErrorCode::Unauthorized) => {
            Some("Check your token with `ta login`, or ask the engine's operator for a new one.")
        }
        Some(ErrorCode::RateLimited) => Some("Wait a moment, then send the line again."),
        Some(ErrorCode::LimitReached) => {
            Some("You can still read your games until the month turns.")
        }
        _ => None,
    };
    (e.to_string(), hint)
}

/// Completion candidates for the input line, from the glance's verbs and
/// their targets. The result is whole replacement lines.
pub fn completions(glance: &Glance, input: &str) -> Vec<String> {
    let Some(rest) = input.strip_prefix('/') else {
        return vec![];
    };
    let words: Vec<(VerbName, &str)> = glance
        .verbs
        .iter()
        .filter(|v| v.available)
        .filter_map(|v| typed_word(v.name).map(|w| (v.name, w)))
        .collect();
    let Some((word, arg)) = rest.split_once(' ') else {
        let lower = rest.to_lowercase();
        return words
            .iter()
            .filter(|(_, w)| w.starts_with(&lower))
            .map(|(_, w)| format!("/{w} "))
            .collect();
    };
    let Some((verb_name, _)) = words.iter().find(|(_, w)| w.eq_ignore_ascii_case(word)) else {
        return vec![];
    };
    let Some(verb) = glance.verb(*verb_name) else {
        return vec![];
    };
    let prefix = format!("/{word} ");
    let filter = |choices: &[String], typed: &str, head: &str| -> Vec<String> {
        let typed = typed.trim_start().to_lowercase();
        choices
            .iter()
            .filter(|c| c.to_lowercase().starts_with(&typed))
            .map(|c| format!("{head}{c}"))
            .collect()
    };
    if *verb_name == VerbName::Throw {
        if let Some((thing, aim)) = arg.split_once(" at ") {
            let aims = verb.aims.clone().unwrap_or_default();
            return filter(&aims, aim, &format!("{prefix}{thing} at "));
        }
    }
    filter(&verb.targets, arg, &prefix)
}

/// The verbs a player can type now, for the input's title.
pub fn verb_hint(glance: &Glance) -> String {
    glance
        .verbs
        .iter()
        .filter(|v| v.available)
        .filter_map(|v| typed_word(v.name))
        .map(|w| format!("/{w}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fake::sample_glance;

    #[test]
    fn completes_verbs_then_targets() {
        let g = sample_glance();
        assert_eq!(completions(&g, "/ta"), vec!["/talk ", "/take "]);
        assert_eq!(completions(&g, "/take f"), vec!["/take Filing press"]);
        assert_eq!(
            completions(&g, "/go "),
            vec!["/go The Causeway Court", "/go The Sea Stair"]
        );
        assert!(completions(&g, "/use ").is_empty(), "use has no targets");
        assert!(completions(&g, "look").is_empty());
    }

    #[test]
    fn check_rolls_read_plainly() {
        let roll = Roll {
            kind: RollKind::Check,
            die: Some(20),
            result: Some(14),
            target: Some(12),
        };
        assert_eq!(roll_line(&roll), "check d20: 14 against 12 (made it)");
        let blow = Roll {
            kind: RollKind::Blow,
            die: None,
            result: Some(4),
            target: None,
        };
        assert_eq!(roll_line(&blow), "blow: 4 damage");
    }
}
