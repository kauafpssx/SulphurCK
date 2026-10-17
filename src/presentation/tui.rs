//! TUI — minimal ratatui dashboard for local runs or daemon attach.

use crate::runner::Stats;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use std::io::stdout;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub fn run_tui(stats: Arc<Stats>, total_lines: u64) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let mut last = stats.checked.load(Ordering::Relaxed);

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let checked = stats.checked.load(Ordering::Relaxed);
            let hits = stats.hits.load(Ordering::Relaxed);
            let fails = stats.fails.load(Ordering::Relaxed);
            let retries = stats.retries.load(Ordering::Relaxed);
            let bans = stats.bans.load(Ordering::Relaxed);
            let errors = stats.errors.load(Ordering::Relaxed);
            let customs = stats.customs.load(Ordering::Relaxed);
            let cpm = stats.cpm.load(Ordering::Relaxed);

            // header
            let header = Paragraph::new(format!(
                "SilverBullet TUI — {checked}/{total_lines} checked  |  CPM {cpm}"
            ))
            .block(Block::default().borders(Borders::ALL).title(" Status "));
            f.render_widget(header, chunks[0]);

            // progress gauge
            let pct = if total_lines > 0 {
                (checked as f64 / total_lines as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(" Progress "))
                .gauge_style(Style::default().fg(Color::Cyan))
                .percent(pct as u16);
            f.render_widget(gauge, chunks[1]);

            // stats
            let stats_text = format!(
                "  Hits: {hits}    Fails: {fails}    Retries: {retries}\n  \
                 Bans: {bans}    Errors: {errors}    Customs: {customs}\n\n  \
                 Press Q to quit / stop."
            );
            let stats_para = Paragraph::new(stats_text)
                .block(Block::default().borders(Borders::ALL).title(" Stats "));
            f.render_widget(stats_para, chunks[2]);

            // footer
            let footer = Paragraph::new("  q = quit | esc = stop run")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }

        // detect if checked hasn't moved for a while and nothing running — still show until q
        let _ = last;
        last = stats.checked.load(Ordering::Relaxed);
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
