// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::state::LoadedState;

pub struct Tags {
    pub all_tags: Vec<String>,
    pub available_tags: Vec<String>,
}

pub type TagsState = LoadedState<Tags>;
