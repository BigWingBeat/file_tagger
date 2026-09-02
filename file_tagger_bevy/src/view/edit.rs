use std::ops::DerefMut;

use bevy::{
    color::palettes::tailwind::{ZINC_600, ZINC_700, ZINC_800, ZINC_900},
    prelude::*,
    ui_widgets::Activate,
};
use file_tagger_internals::{Edit, Tag};

use crate::{
    state::LensState,
    widgets::{button, label, submittable_text_input},
};

pub fn edit_view(mut state: LensState<Edit>) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(10)),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
        }
        BackgroundColor(ZINC_900)
        Children [
            entry_list(state.deref_mut()),
            tag_list(state.deref_mut()),
        ]
    }
}

fn tile(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceEvenly,
            width: px(200),
            height: px(200),
            border_radius: px(4),
            row_gap: px(10),
        }
        BackgroundColor(ZINC_800)
        Children [{children}]
    }
}

/// Ways of adding more entries to be edited
fn add_more_buttons(state: &Edit) -> impl SceneList + use<> {
    let width = px(180);
    bsn_list! [
        // New empty entry, prefilled with tag metatags
        (
            submittable_text_input(
                |mut state: LensState<Edit>| state.create_tag_entry(),
                button(bsn!(label("＋"))),
            )
            Node {
                width,
            }
        ),
        // Open search menu to select existing entries
        (button(bsn!(label("＋ Add From Search"))) Node { width }),
        // Open file picker, prefill with appropriate tags from file metadata
        (button(bsn!(label("＋ Import Files"))) Node { width }),
    ]
}

fn entry_tiles(state: &Edit) -> impl SceneList + use<> {
    state
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            tile(bsn!(
                button(bsn_list![
                    // SVG goes here (currently unsupported by Bevy)
                    // svg(state.data().assets.tag.clone()).dims(48.px()),
                    (
                        label(entry.name.clone())
                        TextFont {
                            weight: FontWeight::MEDIUM,
                            font_size: px(20),
                        }
                    ),
                ])
                Node {
                    flex_direction: FlexDirection::Column,
                    // height: Val::Percent(100.0),
                }
                BackgroundColor({if entry.selected { ZINC_700 } else { ZINC_800 }})
                on(move |_: On<Activate>, mut state: LensState<Edit>| state.toggle_entry_selected(index))
            ))
        })
        .collect::<Vec<_>>()
}

fn placeholder_tile(state: &Edit) -> impl Scene + use<> {
    tile(bsn!(
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(2)),
            justify_content: JustifyContent::SpaceEvenly,
            align_items: AlignItems::Center,
            row_gap: px(10),
        }
        Children [ {add_more_buttons(state)} ]
    ))
}

fn entry_list(state: &mut Edit) -> impl Scene + use<> {
    // Display thumbnails of entries being edited, entries can be selected
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
        }
        Children [
            (
                label("Edit Selected Entries")
                TextFont {
                    weight: FontWeight::BOLD,
                    font_size: px(20),
                }
            ),
            (
                Node {
                    overflow: Overflow::scroll_x(),
                    padding: UiRect::all(px(12)),
                    border: px(1),
                    border_radius: px(4),
                }
                BorderColor::all(ZINC_600)
                Children [
                    {entry_tiles(state)},
                    placeholder_tile(state),
                ]
            ),
            (
                Node {
                    justify_content: JustifyContent::End,
                    column_gap: px(10),
                }
                Children [ {add_more_buttons(state)} ]
            ),
        ]
    }
}

fn tag_search_bar(state: &mut Edit) -> impl Scene + use<> {
    // TODO: Autocomplete for tags?
    bsn! {
        submittable_text_input(
            |mut state: LensState<Edit>| state.try_add_searched_tag_to_selected(),
            button(bsn!(label("＋"))),
        )
        Node { width: px(512) }
    }
}

fn tag_item(tag: &Tag) -> impl Scene {
    let tag_clone = tag.clone();
    bsn! {
        Node
        BackgroundColor(ZINC_800)
        Children [
            (
                button(bsn!(label("－")))
                Node { border: px(0) }
                on(move |_: On<Activate>, mut state: LensState<Edit>| state.remove_tag_from_selected(&tag_clone))
            ),
            label(tag.as_str())
        ]
    }
}

fn tag_search_result(tag: &Tag) -> impl Scene + use<> {
    let tag_clone = tag.clone();
    bsn! {
        button(bsn_list! [
            label("＋"),
            label(tag.as_str().to_owned()),
        ])
        on(move |_: On<Activate>, mut state: LensState<Edit>| {
            if state.add_tag_to_selected(&tag_clone) {
                state.tag_search_bar_state.clear();
            }
        })
        BackgroundColor(ZINC_800)
    }
}

fn tag_list(state: &mut Edit) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
        }
        Children [
            (
                Node {
                    column_gap: px(10),
                }
                Children [
                    (
                        label("Tags")
                        TextFont {
                            weight: FontWeight::BOLD,
                            font_size: px(20),
                        }
                    ),
                    tag_search_bar(state),
                ]
            ),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: px(4),
                    border_radius: px(4),
                    border: px(1),
                    flex_grow: 0.2,
                }
                BorderColor::all(ZINC_600)
                Children [
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                    }
                    Children [
                        {{
                            let tags: Box<dyn SceneList> = if state.tag_search_bar_state.is_empty() {
                                // Empty search bar -> no search results
                                Box::new(state
                                    .intersection_of_tags_of_selected_entries()
                                    .map(tag_item)
                                    .collect::<Vec<_>>())
                            } else {
                                // Some search query entered -> show search results
                                // This does a database lookup (probably expensive), so we use `Either` to avoid doing this unless we need to
                                Box::new(state
                                    .tag_prefix_search_results()
                                    .map(|tag| tag_search_result(&tag))
                                    .collect::<Vec<_>>())
                            };
                            tags
                        }}
                    ]
                ]
            ),
            (
                Node {
                    justify_content: JustifyContent::End,
                    column_gap: px(10),
                }
                Children [
                    ( button(bsn!(label("Save Changes"))) on(|_: On<Activate>, mut state: LensState<Edit>| state.save_changes()) ),
                    ( button(bsn!(label("Cancel"))) on(|_: On<Activate>, mut state: LensState<Edit>| state.cancel()) ),
                ]
            ),
        ]
    }
}
