use std::sync::{Arc, OnceLock};

use file_tagger_internals::{ActiveOverlay, Tag, TransactionImpl};
use miette::Report;
use xilem::{
    FontWeight, ViewCtx, WidgetView,
    core::{NoElement, ViewSequence, fork, one_of::Either},
    masonry::{
        layout::{AsUnit, Dim},
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_700, ZYNC_800},
    },
    style::Style,
    tokio::sync::mpsc::UnboundedSender,
    view::{
        CrossAxisAlignment, FlexExt, FlexSequence, FlexSpacer, MainAxisAlignment, button, flex_col,
        flex_row, label, portal, prose, svg, text_button, text_input, worker_raw,
    },
};

use crate::{
    TransAction, TransReaction, XilemAppState,
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

/// Doing the transaction stuff on another thread is necessary to workaround quirks in the backend APIs
fn transaction_worker(
    state: &mut XilemAppState,
) -> impl ViewSequence<XilemAppState, (), ViewCtx, NoElement> + use<> {
    // On capturing: The only state we are capturing is a clone of the database handle,
    // so it's fine for it to not be rebuilt.
    // It would only be a problem if the database was completely replaced with a new one,
    // but that can only happen in the `launcher` view, and not here in the `edit` view
    let db = state.database.inner_db_handle().clone();
    let sender_dongle = Arc::new(OnceLock::new());
    let sender_dongle2 = sender_dongle.clone();
    worker_raw(
        move |proxy, mut receiver| {
            let db = db.clone();
            let sender_dongle = sender_dongle.clone();
            std::thread::spawn(move || {
                let db = db.clone();
                let transaction = db.transaction().unwrap();
                while let Some(action) = receiver.blocking_recv() {
                    match action {
                        TransAction::Get(key) => {
                            proxy.message(TransReaction::Get(transaction.get(key)));
                        }
                        TransAction::Insert(key, value) => {
                            proxy.message(TransReaction::Insert(transaction.insert(key, value)));
                        }
                        TransAction::Remove(key) => {
                            proxy.message(TransReaction::Remove(transaction.remove(key)));
                        }
                        TransAction::Commit => {
                            proxy.message(TransReaction::Commit(transaction.commit()));
                            break;
                        }
                        TransAction::Rollback => {
                            transaction.rollback();
                            proxy.message(TransReaction::Rollback);
                            break;
                        }
                    }
                }
            });

            async move {
                struct DropGuard(Arc<OnceLock<UnboundedSender<TransAction>>>);
                impl Drop for DropGuard {
                    fn drop(&mut self) {
                        self.0.get().unwrap().send(TransAction::Rollback);
                    }
                }

                // Wait for the task to be aborted, which happens on teardown
                let _guard = DropGuard(sender_dongle);
                loop {
                    xilem::tokio::task::yield_now().await;
                }
            }
        },
        move |state: &mut XilemAppState, sender| {
            sender_dongle2.set(sender.clone()).unwrap();
            state.active_transaction = Some(sender);
        },
        |state: &mut XilemAppState, response| match response {
            TransReaction::Get(result) => todo!(),
            TransReaction::Insert(Err(e))
            | TransReaction::Remove(Err(e))
            | TransReaction::Commit(Err(e)) => {
                state.active_overlay = ActiveOverlay::Error(Report::from_err(e));
            }
            TransReaction::Rollback => todo!(),
            _ => {}
        },
    )
}

pub fn edit(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    fork(
        flex_col((entry_list(state), tag_list(state))).padding(10.px()),
        transaction_worker(state),
    )
}
