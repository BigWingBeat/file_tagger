use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::Dimensions,
        theme::ZYNC_600,
    },
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{FlexExt, flex_row, label, text_button, text_input},
};

pub struct SearchMenuState {
    pub active_folder: String,
    search_text: String,
}

impl Default for SearchMenuState {
    fn default() -> Self {
        Self {
            active_folder: "Folder".to_owned(),
            search_text: String::new(),
        }
    }
}

pub fn active_folder_name(state: &mut SearchMenuState) -> impl WidgetView<SearchMenuState> + use<> {
    label(state.active_folder.clone())
        .weight(FontWeight::BOLD)
        .text_size(20.0)
        .dims(Dimensions::width(Dim::Stretch))
}

pub fn search_bar(state: &mut SearchMenuState) -> impl WidgetView<SearchMenuState> + use<> {
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    flex_row((
        text_input(
            state.search_text.clone(),
            |state: &mut SearchMenuState, text| state.search_text = text,
        )
        .placeholder("Search files by tag")
        .border_width(0.0)
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |_| {})
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
