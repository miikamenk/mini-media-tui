use std::io::Read;
use std::mem;

use mpris::{PlaybackStatus, Player, PlayerFinder};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

const MAX_IMAGE_BYTES: u64 = 1_500_000;

pub struct Media {
    pub id: String,
    pub title: String,
    pub author: String,
    pub dur: Option<u64>,
    pub max_dur: Option<u64>,
    pub is_playing: bool,
    pub volume_pct: Option<u8>,
    pub art_url: Option<String>,
    pub art_state: Option<StatefulProtocol>,
}

impl Media {
    pub fn placeholder(label: &str) -> Self {
        Self {
            id: label.to_string(),
            title: "No media".to_string(),
            author: "-".to_string(),
            dur: None,
            max_dur: None,
            volume_pct: None,
            art_url: None,
            art_state: None,
            is_playing: false,
        }
    }
}

pub struct MediaSource {
    pub block_id: String,
    pub player_id: String,
    pub media: Media,
}

impl MediaSource {
    pub fn new(block_id: &str, player_id: &str) -> Self {
        Self {
            block_id: block_id.to_string(),
            player_id: player_id.to_string(),
            media: Media::placeholder(player_id),
        }
    }

    pub fn find_best_player(&self, finder: &PlayerFinder) -> Option<Player> {
        finder
            .find_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                p.bus_name()
                    .to_lowercase()
                    .contains(&self.player_id.to_lowercase())
            })
            .max_by_key(|p| {
                let is_playing = p
                    .get_playback_status()
                    .map(|s| s == PlaybackStatus::Playing)
                    .unwrap_or(false);
                let position = p.get_position().map(|d| d.as_millis()).unwrap_or(0);
                (is_playing, position)
            })
    }

    pub fn refresh(&mut self, finder: &PlayerFinder, picker: &Picker) {
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

            if let Ok(status) = player.get_playback_status() {
                media.is_playing = status == PlaybackStatus::Playing;
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

    pub fn adjust_volume(&self, finder: &PlayerFinder, delta: i8) {
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

    pub fn set_volume(&self, finder: &PlayerFinder, volume: f64) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            let _ = player.set_volume(volume);
        }
    }

    pub fn play_pause(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            let _ = player.play_pause();
        }
    }

    pub fn previous(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            let _ = player.previous();
        }
    }

    pub fn next(&self, finder: &PlayerFinder) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            let _ = player.next();
        }
    }

    pub fn seek(&self, finder: &PlayerFinder, delta: i64) {
        if self.player_id == "empty" {
            return;
        }
        if let Some(player) = self.find_best_player(finder) {
            let _ = player.seek(delta);
        }
    }

    pub fn seek_to_percent(&self, finder: &PlayerFinder, percent: f64) {
        if self.player_id == "empty" {
            return;
        }
        if let (Some(max_dur), Some(player)) = (self.media.max_dur, self.find_best_player(finder)) {
            if max_dur > 0 {
                let target_secs = (max_dur as f64 * percent / 100.0) as i64;
                if let Ok(current_pos) = player.get_position() {
                    let current_secs = current_pos.as_secs() as i64;
                    let delta = (target_secs - current_secs) * 1_000_000;
                    let _ = player.seek(delta);
                }
            }
        }
    }
}

fn load_thumbnail_from_bytes(picker: &Picker, bytes: &[u8]) -> Option<StatefulProtocol> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    Some(picker.new_resize_protocol(dyn_img))
}

pub fn load_thumbnail(picker: &Picker, url: &str) -> Option<StatefulProtocol> {
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
