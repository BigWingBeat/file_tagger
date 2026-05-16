use file_tagger_internals::SearchResultsState;
use xilem::{
    WidgetView,
    view::{GridParams, grid, grid_item, prose},
};

pub fn search_results(
    state: &mut SearchResultsState,
) -> impl WidgetView<SearchResultsState> + use<> {
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
