// Copyright (c) 2026 Analog Devices, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, Wrap},
    Frame,
};

use crate::app::{App, Focus};
use crate::theme::{symbols, theme, Theme};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if app.output_maximized {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // Output pane (maximized)
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        draw_output_pane(frame, chunks[0], app);
        draw_status_bar(frame, chunks[1], app);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Title bar
                Constraint::Min(10),    // Main content (command list + options)
                Constraint::Length(12), // Output pane
                Constraint::Length(1),  // Status bar
            ])
            .split(area);

        draw_title_bar(frame, chunks[0]);
        draw_main_content(frame, chunks[1], app);
        draw_output_pane(frame, chunks[2], app);
        draw_status_bar(frame, chunks[3], app);
    }
}

fn draw_title_bar(frame: &mut Frame, area: Rect) {
    let theme = theme();
    frame.render_widget(Paragraph::new(" cim").style(theme.title()), area);
}

fn draw_main_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Command list
            Constraint::Min(30),    // Options panel
        ])
        .split(area);

    app.command_list.draw(
        frame,
        chunks[0],
        app.focus == Focus::CommandList,
        app.in_workspace,
    );
    draw_options_panel(frame, chunks[1], app);
}

fn draw_options_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = theme();
    let is_focused = app.focus == Focus::Options;
    let is_disabled = app.command_list.is_selected_disabled(app.in_workspace);

    let selected_label = app
        .command_list
        .entries
        .get(app.command_list.selected)
        .map(|e| e.label.as_str())
        .unwrap_or("Options");

    let title = format!(" {} ", selected_label);

    let border_style = if is_disabled {
        theme.disabled()
    } else if is_focused {
        theme.focus_border()
    } else {
        theme.unfocus_border()
    };

    let title_style = if is_disabled {
        theme.disabled()
    } else if is_focused {
        theme.title()
    } else {
        theme.muted()
    };

    let block = Block::default()
        .title(title)
        .title_style(title_style)
        .borders(Borders::ALL)
        .border_type(Theme::BORDER_TYPE)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if is_disabled {
        let msg =
            Paragraph::new("  Not in a workspace. Run 'cim init' first.").style(theme.disabled());
        frame.render_widget(msg, inner);
    } else if let Some(idx) = app.command_list.selected_panel_index() {
        app.panels[idx].draw_options(frame, inner);
    }
}

fn draw_output_pane(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = theme();

    app.output_pane_height = area.height;
    app.output_pane_width = area.width.saturating_sub(4);

    let is_focused = app.focus == Focus::Output;

    let border_style = if is_focused {
        theme.focus_border()
    } else {
        theme.unfocus_border()
    };

    let title = format!(" {} Output ", symbols::icons::OUTPUT);

    let block = Block::default()
        .title(title)
        .title_style(theme.title())
        .borders(Borders::ALL)
        .border_type(Theme::BORDER_TYPE)
        .border_style(border_style)
        .padding(ratatui::widgets::Padding::new(1, 1, 0, 0));

    let text = if app.output_text.is_empty() {
        Text::styled(
            "Output from cim commands will appear here...",
            theme.placeholder(),
        )
    } else {
        let lines: Vec<Line> = app
            .output_text
            .lines()
            .map(|line| {
                let line_lower = line.to_lowercase();
                let style = if line_lower.contains("error") || line_lower.starts_with("error:") {
                    theme.error()
                } else if line_lower.contains("warning") || line_lower.starts_with("warning:") {
                    theme.warning()
                } else if line_lower.contains("success")
                    || line_lower.contains("completed")
                    || line_lower.contains("done")
                {
                    theme.success()
                } else if line_lower.contains("info:") || line_lower.contains("[info]") {
                    theme.info()
                } else {
                    theme.text()
                };
                Line::from(line.to_string()).style(style)
            })
            .collect();
        Text::from(lines)
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: true })
        .scroll((app.output_scroll, 0));

    frame.render_widget(paragraph, area);

    let scrollbar_area = area.inner(Margin {
        horizontal: 0,
        vertical: 1,
    });
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some(symbols::SCROLL_UP))
            .end_symbol(Some(symbols::SCROLL_DOWN))
            .track_symbol(Some("│"))
            .thumb_symbol("█"),
        scrollbar_area,
        &mut app.output_scrollbar_state,
    );
}

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let theme = theme();

    let mut badges: Vec<Span> = Vec::new();

    match app.focus {
        Focus::CommandList => {
            badges.push(Span::styled("[j/k]", theme.key_badge()));
            badges.push(Span::styled(" navigate ", theme.muted()));
            badges.push(Span::styled("[Enter]", theme.key_badge()));
            badges.push(Span::styled(" select ", theme.muted()));
            badges.push(Span::styled("[`]", theme.key_badge()));
            badges.push(Span::styled(" output ", theme.muted()));
            badges.push(Span::styled("[F]", theme.key_badge()));
            badges.push(Span::styled(" fullscreen ", theme.muted()));
            badges.push(Span::styled("[q]", theme.key_badge()));
            badges.push(Span::styled(" quit", theme.muted()));
        }
        Focus::Options => {
            if let Some(panel) = app.active_panel() {
                let keys = panel.status_keys();
                for (key, desc) in keys {
                    badges.push(Span::styled(key, theme.key_badge()));
                    badges.push(Span::styled(format!(" {} ", desc), theme.muted()));
                }
            }
        }
        Focus::Output => {
            badges.push(Span::styled("[j/k]", theme.key_badge()));
            badges.push(Span::styled(" scroll ", theme.muted()));
            badges.push(Span::styled("[PgUp/PgDn]", theme.key_badge()));
            badges.push(Span::styled(" page ", theme.muted()));
            badges.push(Span::styled("[F]", theme.key_badge()));
            badges.push(Span::styled(
                if app.output_maximized {
                    " restore "
                } else {
                    " fullscreen "
                },
                theme.muted(),
            ));
            badges.push(Span::styled("[Esc]", theme.key_badge()));
            badges.push(Span::styled(" back ", theme.muted()));
        }
    }

    if let Some(ref msg) = app.status_message {
        badges.clear();
        badges.push(Span::styled(msg.clone(), theme.status_bar()));
    }

    let line = Line::from(badges);
    frame.render_widget(Paragraph::new(line), area);
}
