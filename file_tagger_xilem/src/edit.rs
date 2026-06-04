use file_tagger_internals::EditEntry;
use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_700, ZYNC_800},
    },
    style::Style,
    view::{
        FlexSpacer, MainAxisAlignment, button, flex_col, flex_row, portal, prose, svg, text_button,
    },
};

use crate::{XilemAppState, view::container_view};

container_view! {
    fn tile(seq: Seq) {
        flex_col(seq)
            .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .dims(Length::const_px(200.0))
            .background_color(ZYNC_800)
            .corner_radius(Length::const_px(4.0))
    }
}

fn entry_tile(index: usize, entry: &EditEntry) -> impl WidgetView<XilemAppState> {
    tile(
        button(
            flex_col((
                // text_input(entry.name, on_changed),
                prose(format!("{}", entry.id)),
                prose(entry.name.clone()),
                // FlexSpacer::Flex(1.0),
            )),
            move |state: &mut XilemAppState| state.edit.toggle_entry_selected(index),
        )
        .background_color(if entry.selected { ZYNC_700 } else { ZYNC_800 })
        .dims(Dimensions::height(Dim::Stretch)),
    )
}

fn placeholder_tile(state: &XilemAppState) -> impl WidgetView<XilemAppState> {
    tile(
        flex_col((
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
        .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
        .padding(Length::const_px(2.0)),
    )
}

pub fn edit(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    flex_col((
        // Display thumbnails of entries being edited, entries can be selected
        portal(
            flex_row((
                state
                    .edit
                    .entries()
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| entry_tile(i, entry))
                    .collect::<Vec<_>>(),
                placeholder_tile(state),
            ))
            .padding(Length::const_px(12.0))
            .border(ZYNC_600, Length::const_px(1.0))
            .corner_radius(Length::const_px(4.0)),
        )
        .constrain_vertical(true),
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
