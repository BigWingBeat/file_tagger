use file_tagger_internals::{EditEntry, Tag};
use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_700, ZYNC_800},
    },
    style::Style,
    view::{
        FlexExt, FlexSequence, FlexSpacer, MainAxisAlignment, button, flex_col, flex_row, portal,
        prose, svg, text_button, text_input,
    },
};

use crate::{
    XilemAppState,
    view::{container_view, search_input},
};

container_view! {
    fn tile(seq: Seq) {
        flex_col(seq)
            .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .dims(200.px())
            .background_color(ZYNC_800)
            .corner_radius(4.px())
    }
}

/// Ways of adding more entries to be edited
fn add_more_buttons() -> impl FlexSequence<XilemAppState> {
    let width = 175.px();
    (
        // Open search menu to select existing entries
        text_button("＋ Add From Search", |_| {}).dims(Dimensions::width(Dim::Fixed(width))),
        // New empty entry, prefilled with tag metatags
        text_button("＋ Create Tag", |state: &mut XilemAppState| {
            state.generate_entry();
        })
        .dims(Dimensions::width(Dim::Fixed(width))),
        // Open file picker, prefill with appropriate tags from file metadata
        text_button("＋ Import Files", |_| {}).dims(Dimensions::width(Dim::Fixed(width))),
    )
}

fn entry_tile((index, entry): (usize, &EditEntry)) -> impl WidgetView<XilemAppState> + use<> {
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

fn placeholder_tile(state: &XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    tile(
        flex_col((
            svg(state.assets.tag.clone()).dims(48.px()),
            add_more_buttons(),
        ))
        .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
        .padding(2.px()),
    )
}

fn entry_list(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    // Display thumbnails of entries being edited, entries can be selected
    flex_col((
        prose("Edit Selected Entries")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(
            flex_row((
                state
                    .edit
                    .entries()
                    .iter()
                    .enumerate()
                    .map(entry_tile)
                    .collect::<Vec<_>>(),
                placeholder_tile(state),
            ))
            .padding(12.px())
            .border(ZYNC_600, 1.px())
            .corner_radius(4.px()),
        )
        .constrain_vertical(true)
        .must_fill(true),
        flex_row((FlexSpacer::Flex(1.0), add_more_buttons())),
    ))
}

fn tag_search_bar(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    // TODO: Autocomplete for tags?
    search_input(
        text_input(
            state.edit.tag_search_bar_state.clone(),
            |state: &mut XilemAppState, text| state.edit.tag_search_bar_state = text,
        )
        .on_enter(|state: &mut XilemAppState, text| {
            if state.edit.add_tag_to_selected(&text.as_str().into()) {
                state.edit.tag_search_bar_state.clear();
            }
        })
        .placeholder("Search For Tags"),
        text_button("＋", |state: &mut XilemAppState| {
            let tag = state.edit.tag_search_bar_state.as_str().into();
            if state.edit.add_tag_to_selected(&tag) {
                state.edit.tag_search_bar_state.clear();
            }
        }),
        Dimensions::width(Dim::Fixed(512.px())),
    )
}

fn tag_item(tag: &Tag) -> impl WidgetView<XilemAppState> {
    // borrowck shit
    let tag_clone = tag.clone();
    flex_row((
        text_button("－", move |state: &mut XilemAppState| {
            state.edit.remove_tag_from_selected(&tag_clone);
        })
        .border_width(0.px()),
        prose(tag.as_str()),
    ))
    .background(ZYNC_800)
}

fn tag_list(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    flex_col((
        flex_row((
            prose("Tags").weight(FontWeight::BOLD).text_size(20.0),
            tag_search_bar(state),
        )),
        // Intersection of tags applied to all selected entries
        flex_col(
            state
                .edit
                .intersection_of_tags_of_selected_entries()
                .map(tag_item)
                .collect::<Vec<_>>(),
        )
        .gap(4.px())
        .padding(4.px())
        .corner_radius(4.px())
        .border(ZYNC_600, 1.px())
        .flex(0.2),
        flex_row((
            FlexSpacer::Flex(1.0),
            text_button("Save Changes", |state: &mut XilemAppState| {}),
            text_button("Cancel", |state: &mut XilemAppState| state.search_menu()),
        )),
    ))
}

pub fn edit(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    flex_col((entry_list(state), tag_list(state))).padding(10.px())
}
