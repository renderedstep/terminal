//! Words the client shows, shared by the TUI and line mode.

use crate::engine::EngineError;
use crate::protocol::{
    ErrorCode, FinishedEvent, Glance, OutcomeKind, Roll, RollKind, Verb, VerbName,
};

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
        Some(ErrorCode::LimitReached) => {
            Some("You can still read your games until the month turns.")
        }
        _ => None,
    };
    (e.to_string(), hint)
}

/// Completion candidates for the input line, from the glance's verbs and
/// their targets. The result is whole replacement lines.
///
/// Each verb completes as `/word target`, with the word the engine names for
/// it. `use` has no word: its targets are whole attempts, each played by its
/// own line, so `/use ` completes a target to that line, and typing the start
/// of a line completes it too.
pub fn completions(glance: &Glance, input: &str) -> Vec<String> {
    let Some(rest) = input.strip_prefix('/') else {
        return vec![];
    };
    let starts =
        |choice: &str, typed: &str| choice.to_lowercase().starts_with(&typed.to_lowercase());
    let use_lines: Vec<(&String, &String)> = glance
        .verb(VerbName::Use)
        .filter(|v| v.available)
        .map(|v| {
            let lines = v.lines.as_deref().unwrap_or_default();
            v.targets
                .iter()
                .zip(lines)
                .filter_map(|(t, l)| l.as_ref().map(|l| (t, l)))
                .collect()
        })
        .unwrap_or_default();
    let words: Vec<(&Verb, &str)> = glance
        .verbs
        .iter()
        .filter(|v| v.available)
        .filter_map(|v| v.word.as_deref().map(|w| (v, w)))
        .collect();
    let Some((word, arg)) = rest.split_once(' ') else {
        let mut found: Vec<String> = words
            .iter()
            .filter(|(_, w)| starts(w, rest))
            .map(|(_, w)| format!("/{w} "))
            .collect();
        if !use_lines.is_empty() && starts(USE, rest) {
            found.push(format!("/{USE} "));
        }
        found.extend(
            use_lines
                .iter()
                .filter(|(_, l)| starts(l, input))
                .map(|(_, l)| l.to_string()),
        );
        return found;
    };
    let typed = arg.trim_start();
    if word.eq_ignore_ascii_case(USE) {
        return use_lines
            .iter()
            .filter(|(t, _)| starts(t, typed))
            .map(|(_, l)| l.to_string())
            .collect();
    }
    let Some((verb, _)) = words.iter().find(|(_, w)| w.eq_ignore_ascii_case(word)) else {
        return use_lines
            .iter()
            .filter(|(_, l)| starts(l, input))
            .map(|(_, l)| l.to_string())
            .collect();
    };
    let prefix = format!("/{word} ");
    let filter = |choices: &[String], typed: &str, head: &str| -> Vec<String> {
        let typed = typed.trim_start();
        choices
            .iter()
            .filter(|c| starts(c, typed))
            .map(|c| format!("{head}{c}"))
            .collect()
    };
    if verb.name == VerbName::Throw {
        if let Some((thing, aim)) = arg.split_once(" at ") {
            let aims = verb.aims.clone().unwrap_or_default();
            return filter(&aims, aim, &format!("{prefix}{thing} at "));
        }
    }
    filter(&verb.targets, arg, &prefix)
}

/// What the player types before a `use` target to have Tab complete it to
/// the target's line. The engine never sees it.
const USE: &str = "use";

/// The line to send for what the player typed: `/use <target>` becomes the
/// line the engine gave for that target, and anything else is sent as it is.
pub fn line_to_send(glance: &Glance, typed: &str) -> String {
    let target = typed
        .strip_prefix('/')
        .and_then(|rest| rest.split_once(' '))
        .filter(|(word, _)| word.eq_ignore_ascii_case(USE))
        .map(|(_, target)| target.trim());
    let line = target.and_then(|target| {
        let verb = glance.verb(VerbName::Use)?;
        let i = verb
            .targets
            .iter()
            .position(|t| t.eq_ignore_ascii_case(target))?;
        verb.lines.as_ref()?.get(i)?.clone()
    });
    line.unwrap_or_else(|| typed.to_string())
}

/// The verbs a player can type now, for the input's title.
pub fn verb_hint(glance: &Glance) -> String {
    glance
        .verbs
        .iter()
        .filter(|v| v.available)
        .filter_map(|v| match (&v.word, &v.lines) {
            (Some(w), _) => Some(format!("/{w}")),
            (None, Some(_)) if v.name == VerbName::Use => Some(format!("/{USE}")),
            _ => None,
        })
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
        assert_eq!(
            completions(&g, "/use off"),
            vec!["/offer Assize tide-slate to Neb Halloran"],
            "a use target completes to the line that plays it"
        );
        assert_eq!(
            completions(&g, "/of"),
            vec!["/offer Assize tide-slate to Neb Halloran"]
        );
        assert_eq!(completions(&g, "/u"), vec!["/use "]);
        assert!(completions(&g, "look").is_empty());
    }

    #[test]
    fn a_use_target_is_sent_as_its_line() {
        let g = sample_glance();
        assert_eq!(
            line_to_send(
                &g,
                "/use offer assize tide-slate to Neb Halloran; they may refuse"
            ),
            "/offer Assize tide-slate to Neb Halloran"
        );
        assert_eq!(line_to_send(&g, "/use nothing"), "/use nothing");
        assert_eq!(line_to_send(&g, "/go The Sea Stair"), "/go The Sea Stair");
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
