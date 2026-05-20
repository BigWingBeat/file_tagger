//! Functions that define all the top-level views and handle state lensing
use std::fmt::Debug;

use xilem::{
    FontWeight, WidgetView,
    core::lens,
    masonry::{
        layout::{Dim, Length},
        parley::GenericFamily,
        properties::Dimensions,
        theme::{ZYNC_700, ZYNC_800, ZYNC_900},
    },
    palette::css::RED,
    style::{Padding, Style},
    view::{
        Flex, FlexSequence, FlexSpacer, MainAxisAlignment, ZStackSequence, flex_col, flex_row,
        prose, text_button, zstack,
    },
};

use file_tagger_internals::{AppState, DatabaseState};

use crate::{
    edit::edit,
    launcher::launcher,
    search_menu::{import_button, search_bar},
    search_results::search_results,
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

pub fn active_folder_name(state: &mut DatabaseState) -> impl WidgetView<DatabaseState> + use<> {
    prose(state.active_folder().name.to_string_lossy())
        .weight(FontWeight::BOLD)
        .text_size(20.0)
        .dims(Dimensions::width(Dim::Stretch))
}

pub fn overlay_error<Seq, State, E, F>(
    seq: Seq,
    e: Option<&E>,
    callback: F,
) -> impl WidgetView<State> + use<Seq, State, E, F>
where
    F: Fn(&mut State) + Send + Sync + 'static,
    E: Debug + 'static,
    State: 'static,
    Seq: ZStackSequence<State> + Send + Sync,
    Flex<Seq, State>: WidgetView<State>,
    <Flex<Seq, State> as WidgetView<State>>::Widget: Sized,
{
    zstack((
        seq,
        e.map(|e| {
            flex_row(
                flex_col((
                    prose(format!("{e:?}"))
                        .font(GenericFamily::Monospace)
                        .weight(FontWeight::BOLD)
                        .text_size(20.0)
                        .text_color(RED),
                    text_button("Oops", callback),
                ))
                .main_axis_alignment(xilem::view::MainAxisAlignment::Center)
                .background_color(ZYNC_700)
                .border(ZYNC_800, Length::const_px(2.0))
                .corner_radius(Length::const_px(6.0))
                .padding(Padding::all(Length::const_px(16.0)))
                .dims(Dimensions::height(Dim::MinContent)),
            )
            .main_axis_alignment(xilem::view::MainAxisAlignment::Center)
        }),
    ))
}
