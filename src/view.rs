//! Functions that define all the top-level views and handle state lensing

use xilem::{
    WidgetView,
    core::lens,
    masonry::{
        layout::Length,
        theme::{ZYNC_800, ZYNC_900},
    },
    style::Style,
    view::{Flex, FlexSequence, FlexSpacer, MainAxisAlignment, flex_col},
};

use crate::{
    AppState,
    edit::edit,
    launcher::launcher,
    search_menu::{import_button, search_bar},
    search_results::search_results,
    tags_db::active_folder_name,
};

pub fn centered_box<Seq, State>(seq: Seq) -> impl WidgetView<State> + use<Seq, State>
where
    State: 'static,
    Seq: FlexSequence<State>,
    Flex<Seq, State>: WidgetView<State>,
    <Flex<Seq, State> as WidgetView<State>>::Widget: Sized,
{
    // Center the inner `flex_col`
    flex_col(
        // Centered and fixed size
        flex_col(seq)
            .main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .dims((Length::const_px(1000.0), Length::const_px(375.0)))
            .padding(Length::const_px(10.0))
            .background_color(ZYNC_800),
    )
    .main_axis_alignment(MainAxisAlignment::Center)
    .background_color(ZYNC_900)
}

pub fn launcher_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    centered_box((FlexSpacer::Flex(1.0), launcher(state)))
}

pub fn search_menu_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut AppState| {
            &mut state.database
        }),
        search_bar(state),
        import_button(state),
        FlexSpacer::Flex(1.0),
        launcher(state),
    ))
}

pub fn search_results_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut AppState| {
            &mut state.database
        }),
        search_bar(state),
        import_button(state),
        lens(search_results, |state: &mut AppState| {
            &mut state.search_results
        }),
    ))
}

pub fn edit_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    edit(state)
}
