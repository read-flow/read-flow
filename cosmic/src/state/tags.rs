// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::iced::widget::combo_box;

use crate::state::LoadedState;

pub struct Tags {
    pub all_tags: Vec<String>,
    pub available_tags: combo_box::State<String>,
}

pub type TagsState = LoadedState<Tags>;
