// SPDX-License-Identifier: GPL-3.0-or-later

use crate::state::LoadedState;
use cosmic::iced::widget::combo_box;

pub struct Tags {
    pub all_tags: Vec<String>,
    pub available_tags: combo_box::State<String>,
}

pub type TagsState = LoadedState<Tags>;
