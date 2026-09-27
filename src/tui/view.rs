//! Drawing the TUI: the log and input on the left, the glance's panels on
//! the right. Under 80 columns the panels fold into a short status block.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, Entry};
use crate::present;
use crate::protocol::Glance;

const SIDE_WIDTH: u16 = 34;
const NARROW: u16 = 80;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width >= NARROW {
        let [left, right] =
            Layout::horizontal([Constraint::Min(40), Constraint::Length(SIDE_WIDTH)]).areas(area);
        draw_main(frame, app, left, None);
        draw_panels(frame, &app.glance, right);
    } else {
        let status = status_lines(&app.glance);
        draw_main(frame, app, area, Some(status));
    }
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect, status: Option<Vec<Line<'static>>>) {
    let status_height = status.as_ref().map_or(0, |s| s.len() as u16);
    let [log_area, status_area, input_area] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(status_height),
        Constraint::Length(3),
    ])
    .areas(area);

    let title = match &app.glance.room {
        Some(room) => format!(" {} ", room.name),
        None => format!(" {} ", app.title),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner_width = block.inner(log_area).width;
    let inner_height = block.inner(log_area).height;
    let paragraph = Paragraph::new(log_text(app)).wrap(Wrap { trim: false });
    let total = paragraph.line_count(inner_width) as u16;
    let bottom = total.saturating_sub(inner_height);
    let offset = bottom.saturating_sub(app.scroll);
    frame.render_widget(paragraph.block(block).scroll((offset, 0)), log_area);

    if let Some(status) = status {
        frame.render_widget(Paragraph::new(status), status_area);
    }

    let hint = present::verb_hint(&app.glance);
    let mut input_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {hint} "));
    let footer = if app.status.is_empty() {
        app.engine_label.clone()
    } else {
        app.status.clone()
    };
    input_block = input_block.title_bottom(Line::from(format!(" {footer} ")).right_aligned());
    let prompt = Line::from(vec![
        Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(app.input.clone()),
    ]);
    frame.render_widget(Paragraph::new(prompt).block(input_block), input_area);
    let x = input_area.x + 3 + app.input.chars().count() as u16;
    if x < input_area.right().saturating_sub(1) {
        frame.set_cursor_position((x, input_area.y + 1));
    }
}

fn log_text(app: &App) -> Text<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line<'static>> = vec![];
    let push_prose = |lines: &mut Vec<Line<'static>>, text: &str, style: Style| {
        for l in text.lines() {
            lines.push(Line::styled(l.to_string(), style));
        }
    };
    for entry in &app.log {
        if !lines.is_empty() && !matches!(entry, Entry::Outcome(_)) {
            lines.push(Line::raw(""));
        }
        match entry {
            Entry::Typed(t) => lines.push(Line::styled(
                format!("> {t}"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Entry::Prose(t) => push_prose(&mut lines, t, Style::default()),
            Entry::Refusal(t) => push_prose(&mut lines, t, Style::default().fg(Color::Yellow)),
            Entry::Outcome(t) => lines.push(Line::styled(format!("» {t}"), dim)),
            Entry::Notice(t) => push_prose(&mut lines, t, Style::default().fg(Color::Cyan)),
            Entry::Error(t) => push_prose(&mut lines, t, Style::default().fg(Color::Red)),
        }
    }
    if !app.preview.is_empty() {
        lines.push(Line::raw(""));
        push_prose(
            &mut lines,
            &app.preview,
            Style::default().add_modifier(Modifier::ITALIC),
        );
    }
    Text::from(lines)
}

/// Each side panel: title, its lines, and the command that prints them all
/// into the log.
fn panels(g: &Glance) -> Vec<(&'static str, Vec<Line<'static>>, &'static str)> {
    let lines = |v: Vec<String>, empty: &str| -> Vec<Line<'static>> {
        if v.is_empty() {
            vec![Line::styled(
                empty.to_string(),
                Style::default().fg(Color::DarkGray),
            )]
        } else {
            v.into_iter().map(Line::raw).collect()
        }
    };
    let list = present::panel_lists(g);
    let room = match &g.room {
        Some(r) => {
            let mut lines = vec![Line::styled(
                r.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            if let Some(within) = &r.within {
                lines.push(Line::raw(format!("in {within}")));
            }
            lines
        }
        None => vec![Line::styled(
            "nowhere",
            Style::default().fg(Color::DarkGray),
        )],
    };
    let people: Vec<Line<'static>> = if g.people.is_empty() {
        lines(vec![], "nothing")
    } else {
        g.people
            .iter()
            .map(|p| {
                let mut spans = vec![Span::raw(p.name.clone())];
                if p.foe {
                    spans.push(Span::styled(" ⚔", Style::default().fg(Color::LightRed)));
                }
                if let Some(c) = &p.condition {
                    spans.push(Span::styled(
                        format!(" ({c})"),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                Line::from(spans)
            })
            .collect()
    };
    let beat = match (&g.ended, &g.next_beat) {
        (Some(ended), _) if g.over => vec![Line::raw(format!("Over: {ended}"))],
        (_, Some(b)) => vec![Line::raw(b.clone())],
        _ => lines(vec![], "no story beat ahead"),
    };
    vec![
        ("Room", room, ":panels"),
        ("Ways out", lines(list.exits, "nothing"), ":exits"),
        ("Here", people, ":here"),
        ("Lying here", lines(list.lying_here, "nothing"), ":items"),
        ("Carrying", lines(list.carrying, "nothing"), ":carrying"),
        ("Condition", lines(list.condition, "nothing"), ":panels"),
        ("Next beat", beat, ":panels"),
    ]
}

/// The order panels get height in when the side is too short for all of
/// them: an exit left off the screen misleads play, so Ways out comes first.
const PRIORITY: [&str; 7] = [
    "Ways out",
    "Room",
    "Here",
    "Lying here",
    "Carrying",
    "Condition",
    "Next beat",
];

/// The height each panel gets, borders included, given the rows each needs
/// and the height there is. Ways out gets all it needs first, as far as the
/// height goes; then every other panel that fits gets one row, and the rest
/// is handed out in priority order. A panel that cannot get even one row
/// gets no height at all.
pub fn panel_heights(titles: &[&str], needed: &[u16], height: u16) -> Vec<u16> {
    let order: Vec<usize> = PRIORITY
        .iter()
        .filter_map(|t| titles.iter().position(|x| x == t))
        .chain((0..titles.len()).filter(|i| !PRIORITY.contains(&titles[*i])))
        .collect();
    let mut heights = vec![0u16; titles.len()];
    let mut left = height;
    if let Some(&first) = order.first().filter(|i| titles[**i] == PRIORITY[0]) {
        // Leave the room's name a row where there is any height to speak of.
        let cap = if height >= 6 && titles.len() > 1 {
            height - 3
        } else {
            height
        };
        heights[first] = needed[first].min(cap);
        if heights[first] < 3 {
            heights[first] = 0;
        }
        left -= heights[first];
    }
    for &i in &order {
        if heights[i] > 0 {
            continue;
        }
        let min = needed[i].min(3);
        if left >= min {
            heights[i] = min;
            left -= min;
        }
    }
    for &i in &order {
        if heights[i] == 0 {
            continue;
        }
        let more = needed[i].saturating_sub(heights[i]).min(left);
        heights[i] += more;
        left -= more;
    }
    heights
}

/// The lines that fit in `rows` rows at `width`, with a "+N more" line in
/// place of the last one when some do not.
fn fit_lines(
    lines: Vec<Line<'static>>,
    width: usize,
    rows: usize,
    command: &str,
) -> Vec<Line<'static>> {
    let row_count = |l: &Line| l.width().div_ceil(width).max(1);
    if lines.iter().map(row_count).sum::<usize>() <= rows {
        return lines;
    }
    let total = lines.len();
    if rows <= 1 {
        // No row to spare for the marker: it goes on the end of the first line.
        let mut first = lines.into_iter().next().unwrap_or_default();
        first.spans.push(Span::styled(
            format!(" +{} more", total - 1),
            Style::default().fg(Color::Yellow),
        ));
        return vec![first];
    }
    let mut kept = vec![];
    let mut used = 0;
    for line in lines {
        let r = row_count(&line);
        if used + r + 1 > rows {
            break;
        }
        used += r;
        kept.push(line);
    }
    let hidden = total - kept.len();
    kept.push(Line::styled(
        format!("+{hidden} more · {command}"),
        Style::default().fg(Color::Yellow),
    ));
    kept
}

fn draw_panels(frame: &mut Frame, g: &Glance, area: Rect) {
    let panels = panels(g);
    let wrap_width = SIDE_WIDTH.saturating_sub(2).max(1) as usize;
    let needed: Vec<u16> = panels
        .iter()
        .map(|(_, lines, _)| {
            let rows: usize = lines
                .iter()
                .map(|l| l.width().div_ceil(wrap_width).max(1))
                .sum();
            rows as u16 + 2
        })
        .collect();
    let titles: Vec<&str> = panels.iter().map(|(t, _, _)| *t).collect();
    let mut heights = panel_heights(&titles, &needed, area.height);
    // A panel with no room at all is named on a line of its own below the
    // others, so lay them out again one row shorter to make that line.
    if heights.contains(&0) {
        heights = panel_heights(&titles, &needed, area.height.saturating_sub(1));
    }
    let dropped: Vec<&str> = titles
        .iter()
        .zip(&heights)
        .filter(|(_, h)| **h == 0)
        .map(|(t, _)| *t)
        .collect();
    let note_height = u16::from(!dropped.is_empty());
    let mut constraints: Vec<Constraint> = heights.iter().map(|h| Constraint::Length(*h)).collect();
    constraints.push(Constraint::Length(note_height));
    constraints.push(Constraint::Min(0));
    let areas = Layout::vertical(constraints).split(area);
    for ((title, lines, command), (rect, height)) in
        panels.into_iter().zip(areas.iter().zip(&heights))
    {
        if *height == 0 {
            continue;
        }
        let lines = fit_lines(
            lines,
            wrap_width,
            height.saturating_sub(2) as usize,
            command,
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} "));
        frame.render_widget(
            Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
            *rect,
        );
    }
    if note_height == 1 {
        frame.render_widget(
            Paragraph::new(Line::styled(
                match dropped.as_slice() {
                    [one] => format!("+ {one} · :panels"),
                    _ => format!("+{} hidden panels · :panels", dropped.len()),
                },
                Style::default().fg(Color::Yellow),
            )),
            areas[heights.len()],
        );
    }
}

/// The folded panels for a narrow terminal.
fn status_lines(g: &Glance) -> Vec<Line<'static>> {
    let list = |v: Vec<&str>| {
        if v.is_empty() {
            "nothing".to_string()
        } else {
            v.join(", ")
        }
    };
    let here = list(g.people.iter().map(|p| p.name.as_str()).collect());
    vec![
        Line::raw(format!(
            "Ways out: {}  Here: {here}",
            list(g.exits.iter().map(|e| e.name.as_str()).collect())
        )),
        Line::raw(format!(
            "Lying here: {}  Carrying: {}",
            list(g.lying_here.iter().map(|t| t.name.as_str()).collect()),
            list(g.carrying.iter().map(|t| t.name.as_str()).collect())
        )),
        Line::raw(format!(
            "{}  Next: {}",
            g.condition.clone().unwrap_or_default(),
            g.next_beat
                .clone()
                .unwrap_or_else(|| "no story beat ahead".into())
        )),
    ]
}
