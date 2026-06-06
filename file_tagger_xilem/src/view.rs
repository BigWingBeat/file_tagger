//! Functions that define all the top-level views and handle state lensing
use std::{fmt::Debug, ops::DerefMut};

use xilem::{
    FontWeight, WidgetView,
    core::{fork, lens, map_state},
    masonry::{
        layout::{AsUnit, Dim},
        parley::GenericFamily,
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_700, ZYNC_800, ZYNC_900},
    },
    palette::css::{RED, TRANSPARENT},
    style::{Padding, Style},
    view::{
        Button, FlexExt, FlexSpacer, MainAxisAlignment, TextInput, flex_col, flex_row, prose,
        spinner, text_button,
    },
};

use file_tagger_internals::DatabaseState;

use crate::{
    XilemAppState,
    edit::edit,
    launcher::launcher,
    search_menu::{edit_buttons, search_bar},
    search_results::search_results,
};

macro_rules! container_view {
    ($vis:vis fn $fn:ident ( $($param:ident: $ty:ty),* $(,)* ) $body:tt) => {
        $vis fn $fn<Seq, State>($($param: $ty),*) -> impl ::xilem::WidgetView<State> + use<Seq, State>
        where
            State: 'static,
            Seq: ::xilem::view::FlexSequence<State>,
            ::xilem::view::Flex<Seq, State>: ::xilem::WidgetView<State>,
            <::xilem::view::Flex<Seq, State> as ::xilem::WidgetView<State>>::Widget: Sized,
        {
            $body
        }
    };
}

pub(crate) use container_view;

// Main views

container_view! {
    pub fn centered_box(seq: Seq) {
        // Center the inner `flex_col`
        flex_col(
            // Centered and fixed size
            flex_col(seq)
                .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                .dims((1000.px(), 375.px()))
                .padding(10.px())
                .background_color(ZYNC_800),
        )
        .main_axis_alignment(MainAxisAlignment::Center)
        .background_color(ZYNC_900)
    }
}

pub fn launcher_view(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    centered_box((launcher(state), FlexSpacer::Flex(1.0)))
}

pub fn search_menu_view(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut XilemAppState| {
            &mut state.database
        }),
        search_bar(state),
        edit_buttons(state),
        launcher(state),
        FlexSpacer::Flex(1.0),
    ))
}

pub fn search_results_view(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut XilemAppState| {
            &mut state.database
        }),
        search_bar(state),
        edit_buttons(state),
        lens(search_results, |state: &mut XilemAppState| {
            &mut state.search_results
        })
        .flex(1.0),
    ))
}

pub fn edit_view(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    edit(state)
}

pub fn active_folder_name(state: &mut DatabaseState) -> impl WidgetView<DatabaseState> + use<> {
    prose(state.active_folder().name.to_string_lossy())
        .weight(FontWeight::BOLD)
        .text_size(20.0)
        .dims(Dimensions::width(Dim::Stretch))
}

pub fn search_input<State, F, V>(
    text_input: TextInput<State, ()>,
    button: Button<State, (), F, V>,
    dims: Dimensions,
) -> impl WidgetView<State> + use<State, F, V>
where
    Button<State, (), F, V>: WidgetView<State>,
    <Button<State, (), F, V> as WidgetView<State>>::Widget: Sized,
{
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    // TODO: on-hover styling (see: <https://github.com/linebender/xilem/issues/1786>)
    flex_row((
        text_input
            .border_width(0.px())
            .background(TRANSPARENT)
            .flex(1.0),
        button
            .corner_radius(f64::MAX.px()) // circle
            .border_width(0.px())
            // Manually tuned padding to make it look centered and circular
            .padding(Padding {
                top: 5.px(),
                ..Padding::horizontal(8.px())
            }),
    ))
    .gap(1.px())
    // The text input has its own padding, this is just for the button
    .padding(Padding {
        right: 12.px(),
        ..Padding::vertical(2.px())
    })
    // Border and corner radius the same as the text input
    .border(ZYNC_600, 1.px())
    .corner_radius(4.px())
    .dims(dims)
}

// Overlay views

container_view! {
    pub fn centered_flex_box(seq: Seq) {
        // Center the inner `flex_col`
        flex_row(
            // Centered and dynamic size
            flex_col(seq)
                .main_axis_alignment(MainAxisAlignment::Center)
                .background_color(ZYNC_700)
                .border(ZYNC_600, 3.px())
                .corner_radius(6.px())
                .padding(Padding::all(16.px()))
                .dims(Dimensions::height(Dim::MinContent)),
        )
        .main_axis_alignment(MainAxisAlignment::Center)
    }
}

pub fn error_view<State, E, F>(e: &E, callback: F) -> impl WidgetView<State> + use<State, E, F>
where
    F: Fn(&mut State) + Send + Sync + 'static,
    E: Debug + 'static,
    State: 'static,
{
    centered_flex_box((
        prose(format!("{e:?}"))
            .font(GenericFamily::Monospace)
            .weight(FontWeight::BOLD)
            .text_size(20.0)
            .text_color(RED),
        text_button("Oops", callback),
    ))
}

pub fn spinner_view(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    centered_flex_box(fork(
        flex_col((spinner().dims(40.px()), prose("Waiting for dialog..."))),
        state
            .pending_task
            .as_ref()
            .map(|task_view| map_state(task_view(&mut state.state), DerefMut::deref_mut)),
    ))
}
