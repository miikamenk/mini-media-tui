mod app;
mod config;
mod media;
mod ui;
mod widgets;

use std::{io, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mpris::PlayerFinder;
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Overlay};
use config::{save_config, Config, ConfigSource};
use ui::ui;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let finder = PlayerFinder::new().ok();

    loop {
        app.maybe_refresh();
        terminal.draw(|f| ui(f, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match &app.overlay {
                        Overlay::Settings { .. } => match key.code {
                            KeyCode::Tab => {
                                if let Overlay::Settings { slot, cursor } = &mut app.overlay {
                                    *slot = (*slot + 1) % app.sources.len();
                                    *cursor = 0;
                                }
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if let Overlay::Settings { cursor, .. } = &mut app.overlay {
                                    *cursor = (*cursor + 1) % app.available_sources.len();
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if let Overlay::Settings { cursor, .. } = &mut app.overlay {
                                    *cursor = cursor
                                        .checked_sub(1)
                                        .unwrap_or(app.available_sources.len() - 1);
                                }
                            }
                            KeyCode::Enter => {
                                if let Overlay::Settings { slot, cursor } = app.overlay {
                                    if let Some(player) = app.available_sources.get(cursor) {
                                        let display = player
                                            .trim_start_matches("org.mpris.MediaPlayer2.")
                                            .to_string();
                                        app.sources[slot].player_id = display;
                                    }
                                }
                                app.overlay = Overlay::None;
                                let config = Config {
                                    sources: app
                                        .sources
                                        .iter()
                                        .map(|s| ConfigSource {
                                            block_id: s.block_id.clone(),
                                            player_id: s.player_id.clone(),
                                        })
                                        .collect(),
                                    refresh_interval_secs: app.refresh_interval.as_secs(),
                                };
                                let _ = save_config(&config);
                            }
                            KeyCode::Char('s') | KeyCode::Esc | KeyCode::Char('q') => {
                                app.overlay = Overlay::None
                            }
                            _ => {}
                        },
                        Overlay::Help { .. } => match key.code {
                            KeyCode::Char('j') | KeyCode::Down => {
                                if let Overlay::Help { scroll } = &mut app.overlay {
                                    *scroll = scroll.saturating_add(1);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if let Overlay::Help { scroll } = &mut app.overlay {
                                    *scroll = scroll.saturating_sub(1);
                                }
                            }
                            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                                app.toggle_help()
                            }
                            _ => {}
                        },
                        _ => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Char('?') => app.toggle_help(),
                            KeyCode::Char('s') => app.toggle_settings(),
                            KeyCode::Char('h') => app.toggle_selected(),
                            KeyCode::Char('l') => app.toggle_selected(),
                            KeyCode::Char('j') | KeyCode::Down => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.adjust_volume(finder, -5);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.adjust_volume(finder, 5);
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.play_pause(finder);
                                }
                            }
                            KeyCode::Char('p') => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.previous(finder);
                                }
                            }
                            KeyCode::Char('n') => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.next(finder);
                                }
                            }
                            KeyCode::Left => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.seek(finder, -5 * 1000000);
                                }
                            }
                            KeyCode::Right => {
                                if let (Some(source), Some(finder)) =
                                    (app.selected_source_mut(), finder.as_ref())
                                {
                                    source.seek(finder, 5 * 1000000);
                                }
                            }
                            _ => {}
                        },
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let click_rect = ratatui::layout::Rect::new(mouse.column, mouse.row, 1, 1);
                        if let Some(finder) = finder.as_ref() {
                            app.handle_click(finder, click_rect);
                        }
                    }
                    MouseEventKind::ScrollUp => match &app.overlay {
                        Overlay::Help { scroll } => {
                            if let Overlay::Help { scroll } = &mut app.overlay {
                                *scroll = scroll.saturating_sub(1);
                            }
                        }
                        _ => {
                            if let (Some(source), Some(finder)) =
                                (app.selected_source_mut(), finder.as_ref())
                            {
                                source.adjust_volume(finder, 1);
                            }
                        }
                    },
                    MouseEventKind::ScrollDown => match &app.overlay {
                        Overlay::Help { scroll } => {
                            if let Overlay::Help { scroll } = &mut app.overlay {
                                *scroll = scroll.saturating_add(1);
                            }
                        }
                        _ => {
                            if let (Some(source), Some(finder)) =
                                (app.selected_source_mut(), finder.as_ref())
                            {
                                source.adjust_volume(finder, -1);
                            }
                        }
                    },
                    _ => {}
                },
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
