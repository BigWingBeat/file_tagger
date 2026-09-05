use std::ops::DerefMut;

use bevy::prelude::*;
use file_tagger_internals::SearchResults;

use crate::{
    state::LensState,
    view::search_menu::{active_folder_name, edit_buttons, search_bar},
    widgets::label,
};

use super::centered_box;

pub fn search_results_view(mut state: LensState<SearchResults>) -> impl Scene + use<> {
    centered_box(bsn_list![
        active_folder_name(&state.database)
        --
        search_bar(state.deref_mut())
        --
        edit_buttons(state.deref_mut())
        --
        search_results(state.deref_mut())
    ])
}

pub fn search_results(state: &mut SearchResults) -> impl Scene + use<> {
    bsn! {
        Node {
            display: Display::Grid,
            height: Val::Percent(100.0),
        }
        Children [
            {
                state
                .entries()
                .iter()
                .map(|entry| {
                    bsn! {
                        label(entry.name.clone())
                        Node {
                            display: Display::Grid,
                        }
                    }
                })
                .collect::<Vec<_>>()
            }
        ]
    }
}
