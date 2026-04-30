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

pub mod build;
pub mod config;
pub mod docs;
pub mod install;
pub mod release;
pub mod utils;
pub mod workspace;

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::cim::CimCommand;

pub enum PanelAction {
    None,
    Run(CimCommand),
    #[allow(dead_code)]
    Back,
}

pub trait CommandPanel {
    fn draw_options(&mut self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> PanelAction;
    fn status_keys(&self) -> Vec<(&str, &str)>;
    fn check_async(&mut self) {}
}
