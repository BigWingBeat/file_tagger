use std::sync::mpsc::{Receiver, SyncSender};

use file_tagger_internals::{
    Buffer, DatabaseError, DatabaseResult, Tag, TransactionImpl, TransactionResult, UntypedTable,
};
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

pub fn edit(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> {
    fork(
        flex_col((entry_list(state), tag_list(state))).padding(10.px()),
        TransactionWorker,
    )
}

/// This is the message type passed over a channel to allow the GUI to control the transaction
enum TransAction {
    Get(UntypedTable, Buffer),
    Insert(UntypedTable, Buffer, Buffer),
    Remove(UntypedTable, Buffer),
    Commit,
    Rollback,
}

/// This is the message type passed back from the transaction thread with the result of the action.
/// There is no corresponding `Rollback` variant because that is an infallible operation
enum TransReaction {
    Get(Result<Option<Buffer>, DatabaseError>),
    Insert(Result<(), DatabaseError>),
    Remove(Result<(), DatabaseError>),
    Commit(Result<(), DatabaseError>),
}

/// Talks to the transaction thread
pub struct TransactionApi {
    sender: SyncSender<TransAction>,
    receiver: Receiver<TransReaction>,
}

impl TransactionApi {
    /// Used similarly to `Database::create_temporary`, should be replaced when Xilem's enum state ergonomics get better
    pub fn new_disconnected() -> Self {
        Self::new().0
    }

    fn new() -> (Self, Receiver<TransAction>, SyncSender<TransReaction>) {
        // Capacities of 0 because we always wait to get a result back right after sending an action,
        // so it's not possible for multiple messages to get queued up on either channel
        let (action_sender, action_receiver) = std::sync::mpsc::sync_channel(0);
        let (reaction_sender, reaction_receiver) = std::sync::mpsc::sync_channel(0);
        (
            Self {
                sender: action_sender,
                receiver: reaction_receiver,
            },
            action_receiver,
            reaction_sender,
        )
    }
}

impl TransactionApi {
    fn get(&self, table: &UntypedTable, key: impl Into<Buffer>) -> DatabaseResult<Option<Buffer>> {
        self.sender
            .send(TransAction::Get(table.clone(), key.into()))
            .unwrap();
        let TransReaction::Get(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    fn insert(
        &mut self,
        table: &UntypedTable,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> DatabaseResult<()> {
        self.sender
            .send(TransAction::Insert(table.clone(), key.into(), value.into()))
            .unwrap();
        let TransReaction::Insert(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    fn remove(&mut self, table: &UntypedTable, key: impl Into<Buffer>) -> DatabaseResult<()> {
        self.sender
            .send(TransAction::Remove(table.clone(), key.into()))
            .unwrap();
        let TransReaction::Remove(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    fn commit(self) -> DatabaseResult<()> {
        self.sender.send(TransAction::Commit).unwrap();
        let TransReaction::Commit(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    fn rollback(self) {
        self.sender.send(TransAction::Rollback).unwrap();
    }
}

/// Doing the transaction stuff on another thread is necessary to workaround quirks in the backend APIs
struct TransactionWorker;

impl ViewMarker for TransactionWorker {}
impl View<XilemAppState, (), ViewCtx> for TransactionWorker {
    type Element = NoElement;

    type ViewState = SyncSender<TransAction>;

    fn build(
        &self,
        _ctx: &mut ViewCtx,
        app_state: &mut XilemAppState,
    ) -> (Self::Element, Self::ViewState) {
        let (api, receiver, sender) = TransactionApi::new();
        let view_state = api.sender.clone();
        app_state.active_transaction = api;
        let db = app_state.database.inner_db_handle().clone();
        std::thread::spawn(move || {
            let result = db.transaction(|mut transaction| {
                // This must always send a `TransReaction` back after each received `TransAction`, otherwise it will deadlock
                while let Ok(action) = receiver.recv() {
                    match action {
                        TransAction::Get(table, key) => {
                            sender
                                .send(TransReaction::Get(transaction.get(&table, key)))
                                .unwrap();
                        }
                        TransAction::Insert(table, key, value) => {
                            sender
                                .send(TransReaction::Insert(
                                    transaction.insert(&table, key, value),
                                ))
                                .unwrap();
                        }
                        TransAction::Remove(table, key) => {
                            sender
                                .send(TransReaction::Remove(transaction.remove(&table, key)))
                                .unwrap();
                        }
                        TransAction::Commit => return transaction.commit(),
                        TransAction::Rollback => break,
                    }
                }
                // Either we explicitly received a `Rollback` action, or the sender was disconnected
                Ok(transaction.rollback())
            });
            // We eat the results of every non-terminal action, so this is always a `Commit` result
            match result {
                TransactionResult::Ok(()) => sender.send(TransReaction::Commit(Ok(()))).unwrap(),
                TransactionResult::Err(e) => sender.send(TransReaction::Commit(Err(e))).unwrap(),
                TransactionResult::Rollback => {}
            }
        });
        (NoElement, view_state)
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
        view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // `teardown` means this `View` is no longer in the UI tree, which means we have left the `edit` view entirely.
        // This should only be possible by pressing either the "cancel" or "save changes" buttons, both of which
        // should finalize the transaction and terminate the thread on their own, so this is more of a fail-safe.
        let _ = view_state.send(TransAction::Rollback);
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
