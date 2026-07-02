use file_tagger_internals::SearchResults;
use xilem::{
    WidgetView,
    core::lens,
    view::{FlexExt, GridParams, grid, grid_item, prose},
};

use crate::view::centered_box;

pub fn search_results_view(state: &mut SearchResults) -> impl WidgetView<SearchResults> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut SearchResults| {
            &mut state.database
        }),
        search_bar(state),
        edit_buttons(state),
        lens(search_results, |state: &mut SearchResults| {
            &mut state.search_results
        })
        .flex(1.0),
    ))
}

pub fn search_results(state: &mut SearchResults) -> impl WidgetView<SearchResults> + use<> {
    // TODO: see <https://github.com/linebender/xilem/issues/1785>
    let cols = 4;
    grid(
        state
            .entries()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                grid_item(
                    prose(entry.name.clone()),
                    GridParams::new((i % cols) as _, (i / cols) as _, 1, 1),
                )
            })
            .collect::<Vec<_>>(),
        cols as _,
        4,
    )
}
