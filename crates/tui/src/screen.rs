//! The terminal shell — BUILD-SPEC §10's TUI, and nothing that decides
//! anything.
//!
//! Every choice this module makes is delegated to [`crate::Flight`], which is
//! plain data with tested methods. What is left here is: read a key, call a
//! method, draw. That split is why §10's TUI can be held to the same standard
//! as the rest of the product — a terminal event loop is close to untestable,
//! and the answer is to put nothing in it worth testing.

use std::io::{self, Stdout};
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::{Flight, Pane};

/// Restores the terminal when dropped, whatever happened.
///
/// A `Drop` guard rather than a call at the end of `run`: a panic inside the
/// draw loop that left the alternate screen on and raw mode enabled would hand
/// the user a shell they cannot type into, and every subsequent command would
/// look broken. This is the same reason `capture::pty::RawMode` is a guard.
struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Screen {
    fn enter() -> io::Result<Screen> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        out.execute(EnterAlternateScreen)?;
        Ok(Screen {
            terminal: Terminal::new(CrosstermBackend::new(out))?,
        })
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Run the flight recorder until the reader quits.
///
/// **Read-only, absolutely.** There is no key that writes anything: a flight
/// recorder that can be edited from the cockpit is not evidence.
pub fn run(mut flight: Flight) -> io::Result<()> {
    let mut screen = Screen::enter()?;
    screen.terminal.draw(|frame| draw(frame, &flight))?;
    loop {
        // **Draw on change, never on a timer.** The first version redrew every
        // 200 ms whether or not anything had happened, which pins a core on an
        // idle screen — and, less obviously, floods the terminal with repaints
        // fast enough that a reader driving it programmatically can never get a
        // keystroke in. A poll is still used, so a resize repaints without one.
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let ev = event::read()?;
        if matches!(ev, Event::Resize(_, _)) {
            screen.terminal.draw(|frame| draw(frame, &flight))?;
            continue;
        }
        let Event::Key(key) = ev else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if flight.searching {
            match key.code {
                KeyCode::Esc => {
                    flight.searching = false;
                    flight.set_query(String::new());
                }
                KeyCode::Enter => flight.searching = false,
                KeyCode::Backspace => {
                    let mut q = flight.query.clone();
                    q.pop();
                    flight.set_query(q);
                }
                KeyCode::Char(c) => {
                    let mut q = flight.query.clone();
                    q.push(c);
                    flight.set_query(q);
                }
                _ => {}
            }
            screen.terminal.draw(|frame| draw(frame, &flight))?;
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Char('/') => flight.searching = true,
            KeyCode::Char('s') => {
                flight.jump_to_stop();
            }
            KeyCode::Tab => flight.pane = flight.pane.next(),
            KeyCode::Down | KeyCode::Char('j') => flight.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => flight.move_by(-1),
            KeyCode::PageDown => flight.move_by(10),
            KeyCode::PageUp => flight.move_by(-10),
            KeyCode::Home => flight.move_by(isize::MIN / 2),
            KeyCode::End => flight.move_by(isize::MAX / 2),
            _ => {}
        }
        screen.terminal.draw(|frame| draw(frame, &flight))?;
    }
    Ok(())
}

fn draw(frame: &mut Frame, flight: &Flight) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(frame.area());
    header(frame, outer[0], flight);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(outer[1]);
    timeline(frame, body[0], flight);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
        ])
        .split(body[1]);
    detail(frame, right[0], flight);
    claims(frame, right[1], flight);
    promises(frame, right[2], flight);
    footer(frame, outer[2], flight);
}

fn header(frame: &mut Frame, area: Rect, flight: &Flight) {
    let text = Line::from(vec![
        Span::styled(
            format!(" {} ", flight.session),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(flight.headline()),
    ]);
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("thruline")),
        area,
    );
}

fn timeline(frame: &mut Frame, area: Rect, flight: &Flight) {
    let visible = flight.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .filter_map(|i| flight.rows.get(*i))
        .map(|r| {
            let mut spans = vec![
                Span::styled(format!("{:>6} ", r.seq), Style::default()),
                Span::styled(
                    format!("{:<15}", r.kind),
                    Style::default().add_modifier(Modifier::DIM),
                ),
                Span::raw(r.text.clone()),
            ];
            if let Some(risk) = &r.risk {
                spans.push(Span::styled(
                    format!("  ⚑ {risk}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(flight.selected().min(visible.len() - 1)));
    }
    let title = format!("timeline ({}/{})", visible.len(), flight.rows.len());
    frame.render_stateful_widget(
        List::new(items)
            .block(bordered(title, flight.pane == Pane::Timeline))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

fn detail(frame: &mut Frame, area: Rect, flight: &Flight) {
    let lines: Vec<Line> = match flight.current() {
        Some(row) => row
            .detail
            .iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(
                        format!("{k}: "),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(v.clone()),
                ])
            })
            .collect(),
        None => vec![Line::from("nothing matches the search")],
    };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(bordered("detail".to_string(), false)),
        area,
    );
}

fn claims(frame: &mut Frame, area: Rect, flight: &Flight) {
    let items: Vec<ListItem> = flight
        .claims
        .iter()
        .map(|c| {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{:<13}", c.verdict),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(c.text.clone()),
            ])];
            for reason in c.reasons.iter().chain(c.findings.iter()) {
                lines.push(Line::from(Span::styled(
                    format!("    {reason}"),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            ListItem::new(lines)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(bordered(
            "claims → verdict → why".to_string(),
            flight.pane == Pane::Claims,
        )),
        area,
    );
}

fn promises(frame: &mut Frame, area: Rect, flight: &Flight) {
    let items: Vec<ListItem> = if flight.promises.is_empty() {
        vec![ListItem::new(Line::from("none"))]
    } else {
        flight
            .promises
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<10}", p.status),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(p.text.clone()),
                ]))
            })
            .collect()
    };
    frame.render_widget(
        List::new(items).block(bordered(
            "promises".to_string(),
            flight.pane == Pane::Promises,
        )),
        area,
    );
}

fn footer(frame: &mut Frame, area: Rect, flight: &Flight) {
    let text = if flight.searching {
        format!("/{}", flight.query)
    } else {
        format!(
            "stop: {} — {}   ·   ↑↓ move · Tab pane · / search · s jump to stop · q quit",
            flight.stop.reason, flight.stop.detail
        )
    };
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn bordered(title: String, focused: bool) -> Block<'static> {
    let block = Block::default().borders(Borders::ALL).title(title);
    if focused {
        block.border_style(Style::default().add_modifier(Modifier::BOLD))
    } else {
        block
    }
}
