use bevy::prelude::*;
use file_tagger_internals::SearchMenu;

use crate::state::LensState;

use super::centered_box;

pub fn search_menu_view(state: LensState<SearchMenu>) -> impl Scene + use<> {
    centered_box(bsn_list![Text("Text1"), Text("Text2")])
}
