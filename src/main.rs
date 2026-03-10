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
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};

const MAX_IMAGE_BYTES: u64 = 1_500_000;

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
    refresh_interval: Duration,
    last_refresh: Instant,
    picker: Picker,
    selected_media: String,
}

impl App {
    fn new() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let mut app = Self {
            sources: vec![
                MediaSource::new("media_1", "spotify"),
                MediaSource::new("media_2", "firefox"),
            ],
            refresh_interval: Duration::from_secs(1),
            last_refresh: Instant::now(),
            picker,
            selected_media: "spotify".to_string(),
        };
        app.refresh_from_mpris();
        app
    }

    fn refresh_from_mpris(&mut self) {
        if let Ok(finder) = PlayerFinder::new() {
            let picker = &self.picker;
            for source in &mut self.sources {
                source.refresh(&finder, picker);
            }
        } else {
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

fn load_thumbnail(picker: &Picker, url: &str) -> Option<StatefulProtocol> {
    let response = ureq::get(url).call().ok()?;
    let mut reader = response.into_reader();
    let mut limited = reader.by_ref().take(MAX_IMAGE_BYTES);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).ok()?;
    let dyn_img = image::load_from_memory(&bytes).ok()?;
    Some(picker.new_resize_protocol(dyn_img))
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
        if let Some(area) = top_chunks.get(idx) {
            render_media_card(f, source, *area);
        }
    }

    let sys_block = Block::default().title("system_info").borders(Borders::ALL);
    let sys_text = Paragraph::new("press q to quit").block(sys_block);
    f.render_widget(sys_text, main_chunks[1]);
}

fn render_media_card(f: &mut Frame<'_>, source: &mut MediaSource, area: Rect) {
    let block = Block::default()
        .title(source.block_id.as_str())
        .borders(Borders::ALL);
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
        let size = area.height.min(area.width);

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
                match key.code {
                    KeyCode::Char('q') => break,
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

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
