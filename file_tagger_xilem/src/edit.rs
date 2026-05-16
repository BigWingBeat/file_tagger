use xilem::{
    FontWeight, WidgetView,
    masonry::{layout::Length, theme::ZYNC_600},
    style::Style,
    view::{FlexSpacer, flex_col, flex_row, prose, text_button},
};

use file_tagger_internals::AppState;

pub fn edit(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        // Display thumbnails of entries being edited, entries can be selected
        flex_row(
            state
                .edit
                .entries()
                .iter()
                .enumerate()
                .map(|(i, entry)| prose(format!("entry {i}")))
                .collect::<Vec<_>>(),
        )
        .border(ZYNC_600, Length::const_px(1.0))
        .corner_radius(Length::const_px(4.0)),
        // Ways of adding more entries to be edited
        flex_row((
            FlexSpacer::Flex(1.0),
            // New empty entry, prefilled with tag metatags
            text_button("＋ Create Tag", |state: &mut AppState| {
                state.generate_entry();
            }),
            // Open search menu to select existing entries
            text_button("＋ Add Entries", |_| {}),
            // Open file picker, prefill with appropriate tags from file metadata
            text_button("＋ Import Files", |_| {}),
        )),
        prose("Tags").weight(FontWeight::BOLD).text_size(20.0),
        // Intersection of tags applied to all selected entries
        flex_col(FlexSpacer::Flex(1.0))
            .border(ZYNC_600, Length::const_px(1.0))
            .corner_radius(Length::const_px(4.0)),
        flex_row((
            FlexSpacer::Flex(1.0),
            text_button("Save Changes", |state: &mut AppState| {}),
        )),
    ))
}
