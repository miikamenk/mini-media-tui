use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use ratatui_image::{Resize, StatefulImage};

use crate::app::{App, CardRegions};
use crate::media::MediaSource;
use crate::ui::centered_rect;

pub fn render_help_overlay(f: &mut ratatui::Frame<'_>, scroll: u16) {
    let area = centered_rect(50, 60, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let text = vec![
        Line::from(vec![
            Span::styled("h / l", Style::default().fg(Color::Green)),
            Span::raw("   toggle player"),
        ]),
        Line::from(vec![
            Span::styled("j / k", Style::default().fg(Color::Green)),
            Span::raw("  volume down / up"),
        ]),
        Line::from(vec![Span::styled(
            "scroll",
            Style::default().fg(Color::Green),
        )]),
        Line::from(vec![
            Span::styled("␣", Style::default().fg(Color::Green)),
            Span::raw("       play/pause"),
        ]),
        Line::from(vec![
            Span::styled("p", Style::default().fg(Color::Green)),
            Span::raw("       previous"),
        ]),
        Line::from(vec![
            Span::styled("n", Style::default().fg(Color::Green)),
            Span::raw("       next"),
        ]),
        Line::from(vec![
            Span::styled("←", Style::default().fg(Color::Green)),
            Span::raw("       seek backwards 5s"),
        ]),
        Line::from(vec![
            Span::styled("→", Style::default().fg(Color::Green)),
            Span::raw("       seek forwards 5s"),
        ]),
        Line::from(vec![
            Span::styled("s", Style::default().fg(Color::Green)),
            Span::raw("       settings"),
        ]),
        Line::from(vec![
            Span::styled("?", Style::default().fg(Color::Green)),
            Span::raw("       this help"),
        ]),
        Line::from(vec![
            Span::styled("q / Esc", Style::default().fg(Color::Green)),
            Span::raw(" quit"),
        ]),
    ];
    let para = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Left)
        .scroll((scroll, 0));
    f.render_widget(para, area);
}

pub fn render_settings_overlay(f: &mut ratatui::Frame<'_>, app: &App, slot: usize, cursor: usize) {
    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let outer = Block::default()
        .title(" Reassign players ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let halves =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(inner);

    let slot_items: Vec<ListItem> = app
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
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
        })
        .collect();

    let slot_list =
        List::new(slot_items).block(Block::default().title(" Slots ").borders(Borders::ALL));
    let mut slot_state = ListState::default();
    slot_state.select(Some(slot));
    f.render_stateful_widget(slot_list, halves[0], &mut slot_state);

    let player_items: Vec<ListItem> = app
        .available_sources
        .iter()
        .map(|p| {
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
        })
        .collect();

    let player_list = List::new(player_items)
        .block(Block::default().title(" Available ").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Yellow));
    let mut player_state = ListState::default();
    player_state.select(Some(cursor));
    f.render_stateful_widget(player_list, halves[1], &mut player_state);
}

pub fn render_media_card(
    f: &mut ratatui::Frame<'_>,
    source: &mut MediaSource,
    area: Rect,
    is_selected: bool,
    regions: &mut CardRegions,
) {
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
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(15),
            Constraint::Percentage(30),
        ])
        .split(inner);

    render_artwork(f, &mut source.media, chunks[0]);

    let details = Paragraph::new(media_lines(source))
        .style(Style::default())
        .wrap(Wrap { trim: true });
    f.render_widget(details, chunks[1]);

    render_controls(f, source, chunks[2], regions, source.media.is_playing);
}

pub fn render_artwork(f: &mut ratatui::Frame<'_>, media: &mut crate::media::Media, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(state) = media.art_state.as_mut() {
        let h_chunks = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(area.height * 2),
            Constraint::Fill(1),
        ])
        .split(area);

        let v_chunks = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(area.height * 2),
            Constraint::Fill(1),
        ])
        .split(h_chunks[1]);

        let image = StatefulImage::default().resize(Resize::Fit(None));
        f.render_stateful_widget(image, v_chunks[1], state);
    } else {
        let placeholder = Paragraph::new("no artwork")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(placeholder, area);
    }
}

pub fn render_controls(
    f: &mut ratatui::Frame<'_>,
    source: &MediaSource,
    area: Rect,
    regions: &mut CardRegions,
    is_playing: bool,
) {
    let chunks =
        Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)]).split(area);
    let seek_chunks = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[1]);
    let progress_chunk = seek_chunks[0];
    let controls_chunk = chunks[0];
    let volume_chunk = seek_chunks[1];

    let progress_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Blue))
        .label(format!(
            "{} / {}",
            format_duration(source.media.dur),
            format_duration(source.media.max_dur)
        ));
    let ratio = match (source.media.dur, source.media.max_dur) {
        (Some(cur), Some(max)) if max > 0 => (cur as f64 / max as f64).min(1.0),
        _ => 0.0,
    };
    f.render_widget(progress_gauge.ratio(ratio), progress_chunk);
    regions.progress_bar = progress_chunk;

    let vol_ratio = source
        .media
        .volume_pct
        .map(|v| v as f64 / 100.0)
        .unwrap_or(0.0);
    let vol_label = format!("Vol {}%", source.media.volume_pct.unwrap_or(0));
    let vol_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Magenta))
        .label(vol_label)
        .ratio(vol_ratio);
    f.render_widget(vol_gauge, volume_chunk);
    regions.vol_bar = volume_chunk;

    let btn_layout =
        Layout::horizontal([Constraint::Min(6), Constraint::Min(8), Constraint::Min(6)])
            .flex(ratatui::layout::Flex::SpaceAround)
            .split(controls_chunk);

    let btn_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let sel_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    fn make_btn(label: &str, style: Style) -> Paragraph<'_> {
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL))
            .style(style)
    }

    f.render_widget(make_btn("⏮", btn_style), btn_layout[0]);
    regions.prev = btn_layout[0];
    let play_icon = if is_playing { "⏸" } else { "►" };
    f.render_widget(make_btn(play_icon, sel_style), btn_layout[1]);
    regions.play_pause = btn_layout[1];
    f.render_widget(make_btn("⏭", btn_style), btn_layout[2]);
    regions.next = btn_layout[2];
}

pub fn media_lines(source: &MediaSource) -> Vec<Line<'static>> {
    vec![
        Line::from(source.media.title.clone()),
        Line::from(source.media.author.clone()),
    ]
}

pub fn format_duration(value: Option<u64>) -> String {
    value
        .map(|total_secs| {
            let minutes = total_secs / 60;
            let seconds = total_secs % 60;
            format!("{:02}:{:02}", minutes, seconds)
        })
        .unwrap_or_else(|| "--:--".to_string())
}

pub fn format_volume(value: Option<u8>) -> String {
    value
        .map(|v| format!("{v}%"))
        .unwrap_or_else(|| "--%".to_string())
}
