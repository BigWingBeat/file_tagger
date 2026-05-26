use file_tagger_internals::EditEntry;
use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::Length,
        theme::{ZYNC_600, ZYNC_800},
    },
    style::Style,
    view::{FlexSpacer, MainAxisAlignment, button, flex_col, flex_row, prose, svg, text_button},
};

use crate::{XilemAppState, view::container_view};

container_view! {
    fn tile(seq: Seq) {
        flex_col(seq)
            .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .dims(Length::const_px(200.0))
            .padding(Length::const_px(2.0))
            .background_color(ZYNC_800)
            .corner_radius(Length::const_px(4.0))
    }
}

fn entry_tile(entry: &EditEntry) -> impl WidgetView<XilemAppState> {
    tile(button(
        flex_col((
            prose(format!("{}", entry.id)),
            prose(entry.name.clone()),
            FlexSpacer::Flex(1.0),
        )),
        |_| {},
    ))
}

fn placeholder_tile(state: &XilemAppState) -> impl WidgetView<XilemAppState> {
    tile((
        svg(state.assets.tag.clone()).dims(Length::const_px(48.0)),
        // New empty entry, prefilled with tag metatags
        text_button("＋ Create Tag", |state: &mut XilemAppState| {
            state.generate_entry();
        }),
        // Open search menu to select existing entries
        text_button("＋ Add From Search", |_| {}),
        // Open file picker, prefill with appropriate tags from file metadata
        text_button("＋ Import Files", |_| {}),
    ))
}

pub fn edit(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    flex_col((
        // Display thumbnails of entries being edited, entries can be selected
        flex_row((
            state
                .edit
                .entries()
                .iter()
                .enumerate()
                .map(|(i, entry)| entry_tile(entry))
                .collect::<Vec<_>>(),
            placeholder_tile(state),
        ))
        .padding(Length::const_px(10.0))
        .border(ZYNC_600, Length::const_px(1.0))
        .corner_radius(Length::const_px(4.0)),
        // Ways of adding more entries to be edited
        flex_row((
            FlexSpacer::Flex(1.0),
            // New empty entry, prefilled with tag metatags
            text_button("＋ Create Tag", |state: &mut XilemAppState| {
                state.generate_entry();
            }),
            // Open search menu to select existing entries
            text_button("＋ Add From Search", |_| {}),
            // Open file picker, prefill with appropriate tags from file metadata
            text_button("＋ Import Files", |_| {}),
        )),
        prose("Tags").weight(FontWeight::BOLD).text_size(20.0),
        // Intersection of tags applied to all selected entries
        flex_col((FlexSpacer::Fixed(Length::const_px(200.0)),))
            .border(ZYNC_600, Length::const_px(1.0))
            .corner_radius(Length::const_px(4.0)),
        flex_row((
            FlexSpacer::Flex(1.0),
            text_button("Save Changes", |state: &mut XilemAppState| {}),
            text_button("Cancel", |state: &mut XilemAppState| state.search_menu()),
        )),
    ))
    .padding(Length::const_px(10.0))
}
