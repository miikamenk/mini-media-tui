use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use mpris::PlayerFinder;

use crate::config::load_config;
use crate::media::MediaSource;

pub enum Overlay {
    None,
    Help { scroll: u16 },
    Settings { slot: usize, cursor: usize },
}

#[derive(Clone)]
pub struct CardRegions {
    pub card_area: Rect,
    pub play_pause: Rect,
    pub prev: Rect,
    pub next: Rect,
    pub progress_bar: Rect,
    pub vol_bar: Rect,
}

impl CardRegions {
    pub fn default_for(count: usize) -> Vec<Self> {
        vec![
            Self {
                card_area: Rect::default(),
                play_pause: Rect::default(),
                prev: Rect::default(),
                next: Rect::default(),
                progress_bar: Rect::default(),
                vol_bar: Rect::default(),
            };
            count
        ]
    }
}

pub struct App {
    pub sources: Vec<MediaSource>,
    pub available_sources: Vec<String>,
    pub refresh_interval: Duration,
    pub last_refresh: Instant,
    pub picker: Picker,
    pub selected_media: String,
    pub overlay: Overlay,
    pub card_regions: Vec<CardRegions>,
}

impl App {
    pub fn new() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let config = load_config();
        let sources: Vec<MediaSource> = config
            .sources
            .iter()
            .map(|s| MediaSource::new(&s.block_id, &s.player_id))
            .collect();
        let refresh_interval = Duration::from_secs(config.refresh_interval_secs);
        let source_count = config.sources.len();
        let mut app = Self {
            sources,
            available_sources: vec![],
            refresh_interval,
            last_refresh: Instant::now(),
            picker,
            selected_media: "".to_string(),
            overlay: Overlay::None,
            card_regions: CardRegions::default_for(source_count),
        };
        app.refresh_from_mpris();
        app.selected_media = app
            .sources
            .first()
            .map(|s| s.player_id.clone())
            .unwrap_or_default();
        app
    }

    pub fn toggle_help(&mut self) {
        self.overlay = match self.overlay {
            Overlay::None => Overlay::Help { scroll: 0 },
            _ => Overlay::None,
        };
    }

    pub fn toggle_settings(&mut self) {
        self.overlay = match self.overlay {
            Overlay::None => Overlay::Settings { slot: 0, cursor: 0 },
            _ => Overlay::None,
        };
    }

    pub fn refresh_from_mpris(&mut self) {
        if let Ok(finder) = PlayerFinder::new() {
            self.available_sources = finder
                .find_all()
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
                source.media = crate::media::Media::placeholder(&source.player_id);
            }
        }
        self.last_refresh = Instant::now();
    }

    pub fn maybe_refresh(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.refresh_from_mpris();
        }
    }

    pub fn toggle_selected(&mut self) {
        let current_idx = self
            .sources
            .iter()
            .position(|s| s.player_id == self.selected_media);

        self.selected_media = match current_idx {
            Some(idx) => {
                let next_idx = (idx + 1) % self.sources.len();
                self.sources[next_idx].player_id.clone()
            }
            None => self
                .sources
                .first()
                .map(|s| s.player_id.clone())
                .unwrap_or_default(),
        };
    }

    pub fn selected_source_mut(&mut self) -> Option<&mut MediaSource> {
        self.sources
            .iter_mut()
            .find(|s| s.player_id == self.selected_media)
    }

    pub fn handle_click(&mut self, finder: &PlayerFinder, pos: Rect) {
        for (idx, regions) in self.card_regions.iter().enumerate() {
            if pos_in_rect(pos, regions.card_area) {
                self.selected_media = self.sources[idx].player_id.clone();
                if pos_in_rect(pos, regions.play_pause) {
                    if let Some(source) = self.sources.get_mut(idx) {
                        source.play_pause(finder);
                    }
                } else if pos_in_rect(pos, regions.prev) {
                    if let Some(source) = self.sources.get_mut(idx) {
                        source.previous(finder);
                    }
                } else if pos_in_rect(pos, regions.next) {
                    if let Some(source) = self.sources.get_mut(idx) {
                        source.next(finder);
                    }
                } else if pos_in_rect(pos, regions.progress_bar) {
                    if let Some(source) = self.sources.get_mut(idx) {
                        let bar_width = regions.progress_bar.width.saturating_sub(2);
                        if bar_width > 0 {
                            let relative_x = pos.x.saturating_sub(regions.progress_bar.x + 1);
                            let percent = (relative_x as f64 / bar_width as f64) * 100.0;
                            source.seek_to_percent(finder, percent.clamp(0.0, 100.0));
                        }
                    }
                } else if pos_in_rect(pos, regions.vol_bar) {
                    if let Some(source) = self.sources.get_mut(idx) {
                        let bar_width = regions.vol_bar.width;
                        if bar_width > 0 {
                            let relative_x = pos.x.saturating_sub(regions.vol_bar.x);
                            let percent = relative_x as f64 / bar_width as f64;
                            source.set_volume(finder, percent.clamp(0.0, 1.0));
                        }
                    }
                }
                return;
            }
        }
    }
}

pub fn pos_in_rect(pos: Rect, rect: Rect) -> bool {
    pos.x >= rect.x
        && pos.x < rect.x + rect.width
        && pos.y >= rect.y
        && pos.y < rect.y + rect.height
}
