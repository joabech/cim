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

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

use crate::app::{App, Focus};

pub fn handle_event(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            return handle_key_event(app, key);
        }
    }
    Ok(false)
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        app.output_text.clear();
        app.output_scroll = 0;
        app.sync_scrollbar();
        app.status_message = None;
        return Ok(false);
    }

    if key.code == KeyCode::Char('F') {
        app.output_maximized = !app.output_maximized;
        if app.output_maximized {
            app.focus = Focus::Output;
        }
        return Ok(false);
    }

    match app.focus {
        Focus::CommandList => handle_command_list_keys(app, key),
        Focus::Options => handle_options_keys(app, key),
        Focus::Output => handle_output_keys(app, key),
    }
}

fn handle_command_list_keys(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
        KeyCode::Char('j') | KeyCode::Down => app.command_list.select_next(),
        KeyCode::Char('k') | KeyCode::Up => app.command_list.select_previous(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if app.command_list.is_selected_disabled(app.in_workspace) {
                app.status_message = Some("Not in a workspace. Run 'cim init' first.".to_string());
            } else {
                app.focus = Focus::Options;
                app.status_message = None;
            }
        }
        KeyCode::Char('`') => {
            app.focus = Focus::Output;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_options_keys(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
            app.focus = Focus::CommandList;
            return Ok(false);
        }
        KeyCode::Char('`') => {
            app.focus = Focus::Output;
            return Ok(false);
        }
        _ => {}
    }

    if let Some(idx) = app.command_list.selected_panel_index() {
        let action = app.panels[idx].handle_key(key);
        app.handle_panel_action(action);
    }

    Ok(false)
}

fn handle_output_keys(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('`') => {
            if app.output_maximized {
                app.output_maximized = false;
            } else {
                app.focus = Focus::CommandList;
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.scroll_output_down(1),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_output_up(1),
        KeyCode::PageDown => app.scroll_output_down(10),
        KeyCode::PageUp => app.scroll_output_up(10),
        KeyCode::Home | KeyCode::Char('g') => app.scroll_output_top(),
        KeyCode::End | KeyCode::Char('G') => app.scroll_output_bottom(),
        _ => {}
    }
    Ok(false)
}
