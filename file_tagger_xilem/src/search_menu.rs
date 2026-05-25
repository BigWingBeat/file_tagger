use xilem::{
    WidgetView,
    masonry::{layout::Length, theme::ZYNC_600},
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{FlexExt, MainAxisAlignment, flex_row, text_button, text_input},
};

use file_tagger_internals::AppState;

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
        .border_width(Length::const_px(0.0))
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |state: &mut AppState| state.search_results())
            .corner_radius(Length::const_px(f64::MAX)) // circle
            .border_width(Length::const_px(0.0))
            // Manually tuned padding to make it look centered and circular
            .padding(Padding {
                top: Length::const_px(5.0),
                ..Padding::horizontal(Length::const_px(8.5))
            }),
    ))
    .gap(Length::const_px(1.0))
    // The text input has its own padding, this is just for the button
    .padding(Padding {
        right: Length::const_px(12.0),
        ..Padding::vertical(Length::const_px(2.0))
    })
    // Border and corner radius the same as the text input
    .border(ZYNC_600, Length::const_px(1.0))
    .corner_radius(Length::const_px(4.0))
}

pub fn edit_buttons(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_row((
        text_button("Edit Entries", |state: &mut AppState| state.edit_entries()),
        text_button("Import Files", |state: &mut AppState| {
            if let Some(files) = rfd::FileDialog::new()
                .set_title("Select Files to Import")
                .pick_files()
            {
                state.import_files(&files)
            }
        }),
    ))
    .main_axis_alignment(MainAxisAlignment::Center)
}
