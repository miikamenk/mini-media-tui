use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::{App, Overlay};
use crate::widgets::{render_help_overlay, render_media_card, render_settings_overlay};

pub fn ui(f: &mut Frame<'_>, app: &mut App) {
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(f.area());

    for (idx, source) in app.sources.iter_mut().enumerate() {
        let is_selected = source.player_id == app.selected_media;
        if let Some(area) = top_chunks.get(idx) {
            let regions = &mut app.card_regions[idx];
            regions.card_area = *area;
            render_media_card(f, source, *area, is_selected, regions);
        }
    }

    match &app.overlay {
        Overlay::Help { scroll } => render_help_overlay(f, *scroll),
        Overlay::Settings { slot, cursor } => render_settings_overlay(f, app, *slot, *cursor),
        Overlay::None => {}
    }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
