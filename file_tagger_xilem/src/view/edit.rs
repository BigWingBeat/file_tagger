use file_tagger_internals::{Edit, Tag, TransactionHandle};
use xilem::{
    FontWeight, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, NoElement, View, ViewMarker, fork, one_of::Either},
    masonry::{
        layout::{AsUnit, Dim},
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_700, ZYNC_800},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, FlexSequence, FlexSpacer, MainAxisAlignment, button, flex_col,
        flex_row, label, portal, prose, svg, text_button, text_input,
    },
};

use crate::{
    XilemAppState,
    view::{container_view, submittable_text_input},
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
fn add_more_buttons(state: &XilemAppState) -> impl FlexSequence<XilemAppState> + use<> {
    let width = 175.px();
    (
        // New empty entry, prefilled with tag metatags
        submittable_text_input(
            text_input(
                state.edit.tag_create_name_state.clone(),
                |state: &mut XilemAppState, text| state.edit.tag_create_name_state = text,
            )
            .placeholder("Create Tag")
            .on_enter(|state: &mut XilemAppState, _text| state.edit_create_tag_entry()),
            text_button("＋", |state: &mut XilemAppState| {
                state.edit_create_tag_entry();
            }),
            Dimensions::width(Dim::Fixed(width)),
        ),
        // Open search menu to select existing entries
        text_button("＋ Add From Search", |_| {}).dims(Dimensions::width(Dim::Fixed(width))),
        // Open file picker, prefill with appropriate tags from file metadata
        text_button("＋ Import Files", |_| {}).dims(Dimensions::width(Dim::Fixed(width))),
    )
}

fn entry_tiles(state: &XilemAppState) -> impl FlexSequence<XilemAppState> + use<> {
    state
        .edit
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            tile(
                button(
                    flex_col((
                        svg(state.assets.tag.clone()).dims(48.px()),
                        label(entry.name.clone())
                            .weight(FontWeight::MEDIUM)
                            .text_size(20.0),
                    ))
                    // .main_axis_alignment(MainAxisAlignment::Center)
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                    move |state: &mut XilemAppState| state.edit.toggle_entry_selected(index),
                )
                .background_color(if entry.selected { ZYNC_700 } else { ZYNC_800 })
                .dims(Dimensions::height(Dim::Stretch)),
            )
        })
        .collect::<Vec<_>>()
}

fn placeholder_tile(state: &XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    tile(
        flex_col((add_more_buttons(state),))
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
            flex_row((entry_tiles(state), placeholder_tile(state)))
                .padding(12.px())
                .border(ZYNC_600, 1.px())
                .corner_radius(4.px()),
        )
        .constrain_vertical(true)
        .must_fill(true),
        flex_row((FlexSpacer::Flex(1.0), add_more_buttons(state))),
    ))
}

fn tag_search_bar(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    // TODO: Autocomplete for tags?
    submittable_text_input(
        text_input(
            state.edit.tag_search_bar_state.clone(),
            |state: &mut XilemAppState, text| state.edit.tag_search_bar_state = text,
        )
        .on_enter(|state: &mut XilemAppState, _text| {
            state.edit_try_add_searched_tag_to_selected();
        })
        .placeholder("Search For Tags"),
        text_button("＋", |state: &mut XilemAppState| {
            state.edit_try_add_searched_tag_to_selected();
        }),
        Dimensions::width(Dim::Fixed(512.px())),
    )
}

fn tag_item(tag: &Tag) -> impl WidgetView<XilemAppState> + use<> {
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

fn tag_search_result(tag: &Tag) -> impl WidgetView<XilemAppState> + use<> {
    // borrowck shit
    let tag_clone = tag.clone();
    button(
        // TODO: Show number of entries the tag is already applied to?
        flex_row((prose("＋"), prose(tag.as_str()))),
        move |state: &mut XilemAppState| {
            if state.edit.add_tag_to_selected(&tag_clone) {
                state.edit.tag_search_bar_state.clear();
            }
        },
    )
    .background(ZYNC_800)
}

fn tag_list(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    flex_col((
        flex_row((
            prose("Tags").weight(FontWeight::BOLD).text_size(20.0),
            tag_search_bar(state),
        )),
        flex_col(if state.edit.tag_search_bar_state.is_empty() {
            // Empty search bar -> no search results
            Either::A(
                flex_col(
                    state
                        .edit
                        .intersection_of_tags_of_selected_entries()
                        .map(tag_item)
                        .collect::<Vec<_>>(),
                )
                .gap(4.px()),
            )
        } else {
            // Some search query entered -> show search results
            // This does a database lookup (probably expensive), so we use `Either` to avoid doing this unless we need to
            Either::B(
                flex_col(
                    state
                        .edit_tag_prefix_search_results()
                        .map(|tag| tag_search_result(&tag))
                        .collect::<Vec<_>>(),
                )
                .gap(4.px()),
            )
        })
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

pub fn edit(state: &mut Edit) -> impl WidgetView<Edit> {
    fork(
        flex_col((entry_list(state), tag_list(state))).padding(10.px()),
        TransactionWorker,
    )
}

/// Doing the transaction stuff on another thread is necessary to workaround quirks in the backend APIs
struct TransactionWorker;

impl ViewMarker for TransactionWorker {}
impl View<XilemAppState, (), ViewCtx> for TransactionWorker {
    type Element = NoElement;

    type ViewState = TransactionHandle;

    fn build(
        &self,
        _ctx: &mut ViewCtx,
        app_state: &mut XilemAppState,
    ) -> (Self::Element, Self::ViewState) {
        let db = app_state.database.inner_db_handle().clone();
        let (api, handle) = db.initialize_transaction();
        app_state.active_transaction = api;
        (NoElement, handle)
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut XilemAppState,
    ) {
        // `rebuild` is for handling changes in state, but there are no possible state changes we care about,
        // so we have nothing to do here.
        // The only relevant state change would be the database being entirely replaced with a new one,
        // but that can only happen in the `launcher` view, and not here in the `edit` view, so it's fine.
        // The database type is not diffable anyway, so we couldn't handle that case even if we wanted to.
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // `teardown` means this `View` is no longer in the UI tree, which means we have left the `edit` view entirely.
        // This should only be possible by pressing either the "cancel" or "save changes" buttons, both of which
        // should finalize the transaction and terminate the thread on their own. As a failsafe, the `ViewState` type
        // will be dropped shortly after this function returns, which will attempt to finalize the transaction anyway.
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut XilemAppState,
    ) -> MessageResult<()> {
        eprintln!(
            "Message arrived in TransactionWorker::message, but TransactionWorker doesn't consume any messages, this is a bug. {message:?}"
        );
        MessageResult::Stale
    }
}
