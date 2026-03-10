use std::{
    io::{self, Read},
    mem,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mpris::{PlayerFinder, PlaybackStatus, Player};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap, Clear, ListItem, List, ListState},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};

const MAX_IMAGE_BYTES: u64 = 1_500_000;

enum Overlay {
    None,
    Help,
    Settings { slot: usize, cursor: usize },
}

struct Media {
    id: String,
    title: String,
    author: String,
    dur: Option<u64>,
    max_dur: Option<u64>,
    volume_pct: Option<u8>,
    art_url: Option<String>,
    art_state: Option<StatefulProtocol>,
}

impl Media {
    fn placeholder(label: &str) -> Self {
        Self {
            id: label.to_string(),
            title: "No media".to_string(),
            author: "-".to_string(),
            dur: None,
            max_dur: None,
            volume_pct: None,
            art_url: None,
            art_state: None,
        }
    }
}

struct MediaSource {
    block_id: String,
    player_id: String,
    media: Media,
}

impl MediaSource {
    fn new(block_id: &str, player_id: &str) -> Self {
        Self {
            block_id: block_id.to_string(),
            player_id: player_id.to_string(),
            media: Media::placeholder(player_id),
        }
    }

    fn find_best_player(&self, finder: &PlayerFinder) -> Option<Player> {
        finder.find_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                p.bus_name()
                    .to_lowercase()
                    .contains(&self.player_id.to_lowercase())
            })
            .max_by_key(|p| {
                let is_playing = p.get_playback_status()
                    .map(|s| s == PlaybackStatus::Playing)
                    .unwrap_or(false);
                let position = p.get_position()
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                (is_playing, position)
            })
    }

    fn refresh(&mut self, finder: &PlayerFinder, picker: &Picker) {
        if self.player_id == "empty" {
            self.media = Media::placeholder("empty");
            return;
        }

        let mut previous = mem::replace(&mut self.media, Media::placeholder(&self.player_id));
        let mut media = Media::placeholder(&self.player_id);
        let mut new_art_url: Option<String> = None;

        if let Some(player) = self.find_best_player(finder) {
            if let Ok(metadata) = player.get_metadata() {
                if let Some(track_id) = metadata.track_id() {
                    media.id = track_id.to_string();
                }
                if let Some(title) = metadata.title() {
                    media.title = title.to_string();
                }
                if let Some(artists) = metadata.artists() {
                    let combined = artists.join(", ");
                    if !combined.is_empty() {
                        media.author = combined;
                    }
                }
                if let Some(length) = metadata.length() {
                    media.max_dur = Some(length.as_secs());
                }
                new_art_url = metadata.art_url().map(|s| s.to_string());
            }

            if let Ok(position) = player.get_position() {
                media.dur = Some(position.as_secs());
            }

            if let Ok(volume) = player.get_volume() {
                let pct = (volume * 100.0).round().clamp(0.0, 100.0) as u8;
                media.volume_pct = Some(pct);
            }
        }

        match new_art_url {
            Some(url) => {
                media.art_url = Some(url.clone());
                if previous.art_url.as_deref() == Some(url.as_str()) {
                    media.art_state = previous.art_state.take();
                } else if let Some(state) = load_thumbnail(picker, &url) {
                    media.art_state = Some(state);
                }
            }
            None => {
                media.art_state = None;
            }
        }


        self.media = media;
    }

    fn adjust_volume(&self, finder: &PlayerFinder, delta: i8) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            if let Ok(current) = player.get_volume() {
                let new_volume = (current + delta as f64 / 100.0).clamp(0.0, 1.0);
                let _ = player.set_volume(new_volume);
            }
        }
    }

    fn play_pause(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            player.play_pause();
        }
    }

    fn previous(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            player.previous();
        }
    }

    fn next(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            player.next();
        }
    }

    fn seek(&self, finder: &PlayerFinder, delta: i64) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            player.seek(delta);
        }
    }
}

struct App {
    sources: Vec<MediaSource>,
    available_sources: Vec<String>,
    refresh_interval: Duration,
    last_refresh: Instant,
    picker: Picker,
    selected_media: String,
    overlay: Overlay,
}

impl App {
    fn new() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let mut app = Self {
            sources: vec![
                MediaSource::new("media_1", "spotify"),
                MediaSource::new("media_2", "firefox"),
            ],
            available_sources: vec![],
            refresh_interval: Duration::from_secs(1),
            last_refresh: Instant::now(),
            picker,
            selected_media: "".to_string(),
            overlay: Overlay::None,
        };
        app.refresh_from_mpris();
        app
    }

    fn toggle_help(&mut self) {
        self.overlay = match self.overlay {
            Overlay::None => Overlay::Help,
            _ => Overlay::None,
        };
    }

    fn toggle_settings(&mut self) {
        self.overlay = match self.overlay {
            Overlay::None => Overlay::Settings { slot: 0, cursor: 0 },
            _ => Overlay::None,
        };
    }

    fn refresh_from_mpris(&mut self) {
        if let Ok(finder) = PlayerFinder::new() {
            self.available_sources = finder.find_all()
                .unwrap_or_default()
                .iter()
                .map(|p| p.bus_name().to_string())
                .collect();
            let picker = &self.picker;
            for source in &mut self.sources {
                source.refresh(&finder, picker);
            }
        } else {
            self.available_sources.clear();
            for source in &mut self.sources {
                source.media = Media::placeholder(&source.player_id);
            }
        }
        self.last_refresh = Instant::now();
    }

    fn maybe_refresh(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.refresh_from_mpris();
        }
    }

    fn toggle_selected(&mut self) {
        let current_idx = self.sources
            .iter()
            .position(|s| s.player_id == self.selected_media);

        self.selected_media = match current_idx {
            Some(idx) => {
                let next_idx = (idx + 1) % self.sources.len();
                self.sources[next_idx].player_id.clone()
            }
            None => self.sources.first()
                .map(|s| s.player_id.clone())
                .unwrap_or_default(),
        };
    }

    fn selected_source(&self) -> Option<&MediaSource> {
        self.sources.iter().find(|s| s.player_id == self.selected_media)
    }

    fn selected_source_mut(&mut self) -> Option<&mut MediaSource> {
        self.sources.iter_mut().find(|s| s.player_id == self.selected_media)
    }
}

fn load_thumbnail_from_bytes(picker: &Picker, bytes: &[u8]) -> Option<StatefulProtocol> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    Some(picker.new_resize_protocol(dyn_img))
}

fn load_thumbnail(picker: &Picker, url: &str) -> Option<StatefulProtocol> {
    if url.starts_with("file://") {
        let path = url.trim_start_matches("file://");
        let bytes = std::fs::read(path).ok()?;
        return load_thumbnail_from_bytes(picker, &bytes);
    }
    let response = ureq::get(url).call().ok()?;
    let mut reader = response.into_reader();
    let mut limited = reader.by_ref().take(MAX_IMAGE_BYTES);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).ok()?;
    load_thumbnail_from_bytes(picker, &bytes)
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(90), Constraint::Percentage(10)])
        .split(f.area());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    for (idx, source) in app.sources.iter_mut().enumerate() {
        let is_selected = source.player_id == app.selected_media;
        if let Some(area) = top_chunks.get(idx) {
            render_media_card(f, source, *area, is_selected);
        }
    }

    let sys_block = Block::default().title("system_info").borders(Borders::ALL);
    let sys_text = Paragraph::new("press q to quit").block(sys_block);
    f.render_widget(sys_text, main_chunks[1]);

    match &app.overlay {
        Overlay::Help => render_help_overlay(f),
        Overlay::Settings { slot, cursor } => render_settings_overlay(f, app, *slot, *cursor),
        Overlay::None => {}
    }
}

fn render_help_overlay(f: &mut Frame<'_>) {
    let area = centered_rect(50, 60, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let text = vec![
        Line::from(vec![Span::styled("h / l", Style::default().fg(Color::Green)), Span::raw("   toggle player")]),
        Line::from(vec![Span::styled("k / j", Style::default().fg(Color::Green)), Span::raw("   volume up / down")]),
        Line::from(vec![Span::styled("s",     Style::default().fg(Color::Green)), Span::raw("       settings")]),
        Line::from(vec![Span::styled("?",     Style::default().fg(Color::Green)), Span::raw("       this help")]),
        Line::from(vec![Span::styled("q",     Style::default().fg(Color::Green)), Span::raw("       quit")]),
    ];
    let para = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(para, area);
}

fn render_settings_overlay(f: &mut Frame<'_>, app: &App, slot: usize, cursor: usize) {
    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let outer = Block::default()
        .title(" Reassign players ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let halves = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ]).split(inner);

    // Left: slots
    let slot_items: Vec<ListItem> = app.sources.iter().enumerate().map(|(i, s)| {
        let active = i == slot;
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(vec![
            Span::styled(if active { "▶ " } else { "  " }, style),
            Span::styled(format!("{}: {}", s.block_id, s.player_id), style),
        ]))
    }).collect();

    let slot_list = List::new(slot_items)
        .block(Block::default().title(" Slots ").borders(Borders::ALL));
    let mut slot_state = ListState::default();
    slot_state.select(Some(slot));
    f.render_stateful_widget(slot_list, halves[0], &mut slot_state);

    let player_items: Vec<ListItem> = app.available_sources.iter().map(|p| {
        let display = p.trim_start_matches("org.mpris.MediaPlayer2.");
        let display = display.split('.').next().unwrap_or(display);
        let is_current = app.sources[slot].player_id == display;
        let style = if is_current {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(vec![
            Span::styled(if is_current { "✓ " } else { "  " }, style),
            Span::styled(display.to_string(), style),
        ]))
    }).collect();

    let player_list = List::new(player_items)
        .block(Block::default().title(" Available ").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Yellow));
    let mut player_state = ListState::default();
    player_state.select(Some(cursor));
    f.render_stateful_widget(player_list, halves[1], &mut player_state);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]).split(vertical[1])[1]
}

fn render_media_card(f: &mut Frame<'_>, source: &mut MediaSource, area: Rect, is_selected: bool) {
    let border_style = if is_selected {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let block = Block::default()
        .title(source.block_id.as_str())
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner);

    let details = Paragraph::new(media_lines(source))
        .style(Style::default())
        .wrap(Wrap { trim: true });
    f.render_widget(details, chunks[1]);

    render_artwork(f, &mut source.media, chunks[0]);
}

fn render_artwork(f: &mut Frame<'_>, media: &mut Media, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(state) = media.art_state.as_mut() {
        let h_chunks = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(area.height * 2),
            Constraint::Fill(1),
        ]).split(area);

        let v_chunks = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(area.height * 2),
            Constraint::Fill(1),
        ]).split(h_chunks[1]);

        let image = StatefulImage::default().resize(Resize::Fit(None));
        f.render_stateful_widget(image, v_chunks[1], state);
    } else {
        let placeholder = Paragraph::new("no artwork")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(placeholder, area);
    }
}

fn media_lines(source: &MediaSource) -> Vec<Line<'static>> {
    vec![
        //Line::from(Span::styled(
        //    format!("Player: {}", source.player_id),
        //    Style::default().add_modifier(Modifier::BOLD),
        //)),
        //Line::from(format!("Track ID: {}", source.media.id)),
        Line::from(format!("Title: {}", source.media.title)),
        Line::from(format!("Author: {}", source.media.author)),
        Line::from(format!(
            "Progress: {} / {}",
            format_duration(source.media.dur),
            format_duration(source.media.max_dur)
        )),
        Line::from(format!(
            "Volume: {}",
            format_volume(source.media.volume_pct)
        )),
        //Line::from(format!(
        //    "Artwork: {}",
        //    source.media.art_url.as_deref().unwrap_or("(none)")
        //)),
    ]
}

fn format_duration(value: Option<u64>) -> String {
    value
        .map(|total_secs| {
            let minutes = total_secs / 60;
            let seconds = total_secs % 60;
            format!("{:02}:{:02}", minutes, seconds)
        })
        .unwrap_or_else(|| "--:--".to_string())
}

fn format_volume(value: Option<u8>) -> String {
    value
        .map(|v| format!("{v}%"))
        .unwrap_or_else(|| "--%".to_string())
}

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
            if let Event::Key(key) = event::read()? {
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
                                *cursor = cursor.checked_sub(1).unwrap_or(app.available_sources.len() - 1);
                            }
                        }
                        KeyCode::Enter => {
                            if let Overlay::Settings { slot, cursor } = app.overlay {
                                if let Some(player) = app.available_sources.get(cursor) {
                                    let display = player.trim_start_matches("org.mpris.MediaPlayer2.").to_string();
                                    app.sources[slot].player_id = display;
                                }
                            }
                            app.overlay = Overlay::None;
                        }
                        KeyCode::Char('s') | KeyCode::Esc | KeyCode::Char('q') => app.overlay = Overlay::None,
                        _ => {}
                    },
                    _ => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('?') => app.toggle_help(),
                        KeyCode::Char('s') => app.toggle_settings(),
                        KeyCode::Char('h') => app.toggle_selected(),
                        KeyCode::Char('l') => app.toggle_selected(),
                        KeyCode::Char('j') | KeyCode::Down => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.adjust_volume(finder, -5);
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.adjust_volume(finder, 5);
                            }
                        }
                        KeyCode::Char(' ') => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.play_pause(finder);
                            }
                        }
                        KeyCode::Char('p') => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.previous(finder);
                            }
                        }
                        KeyCode::Char('n') => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.next(finder);
                            }
                        }
                        KeyCode::Left => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.seek(finder, -5 * 1000000);
                            }
                        }
                        KeyCode::Right => {
                            if let (Some(source), Some(finder)) = (app.selected_source_mut(), finder.as_ref()) {
                                source.seek(finder, 5 * 1000000);
                            }
                        }
                        _ => {}
                    }
                }
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
