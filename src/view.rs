//! Functions that define all the top-level views and handle state lensing

use xilem::{
    WidgetView,
    core::lens,
    masonry::{
        layout::Length,
        theme::{ZYNC_800, ZYNC_900},
    },
    style::Style,
    view::{Flex, FlexSequence, MainAxisAlignment, flex_col},
};

use crate::{
    AppState,
    launcher::launcher,
    search_menu::{active_folder_name, search_bar},
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
            .padding(10.0)
            .background_color(ZYNC_800),
    )
    .main_axis_alignment(MainAxisAlignment::Center)
    .background_color(ZYNC_900)
}

pub fn launcher_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    centered_box(lens(launcher, |state: &mut AppState| &mut state.launcher))
}

pub fn search_menu_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut AppState| {
            &mut state.search_menu
        }),
        lens(search_bar, |state: &mut AppState| &mut state.search_menu),
        lens(launcher, |state: &mut AppState| &mut state.launcher),
    ))
}
