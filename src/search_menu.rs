use xilem::{
    WidgetView,
    masonry::{layout::Length, theme::ZYNC_600},
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{FlexExt, flex_row, text_button, text_input},
};

use crate::AppState;

#[derive(Default)]
pub struct SearchBarState {
    search_text: String,
}

pub fn search_bar(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    // TODO: on-hover styling (see: <https://github.com/linebender/xilem/issues/1786>)
    flex_row((
        text_input(
            state.search_menu.search_text.clone(),
            |state: &mut AppState, text| state.search_menu.search_text = text,
        )
        .on_enter(|state: &mut AppState, _| state.search_results())
        .placeholder("Search database by tag")
        .border_width(0.0)
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |state: &mut AppState| state.search_results())
            .corner_radius(f64::INFINITY) // circle
            .border_width(0.0)
            // Manually tuned padding to make it look centered and circular
            .padding(Padding {
                top: 5.0,
                ..Padding::horizontal(8.5)
            }),
    ))
    .gap(Length::const_px(1.0))
    // The text input has its own padding, this is just for the button
    .padding(Padding {
        right: 12.0,
        ..Padding::vertical(2.0)
    })
    // Border and corner radius the same as the text input
    .border(ZYNC_600, 1.0)
    .corner_radius(4.0)
}

pub fn import_button(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    text_button("Import Files", |state: &mut AppState| state.import_files())
}
