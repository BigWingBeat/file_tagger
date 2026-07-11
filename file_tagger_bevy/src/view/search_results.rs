use bevy::prelude::*;
use file_tagger_internals::SearchResults;

use crate::state::LensState;

use super::centered_box;

pub fn search_results_view(state: LensState<SearchResults>) -> impl Scene + use<> {
    centered_box(bsn_list![Text("Text1"), Text("Text2")])
}
