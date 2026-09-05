use std::ops::DerefMut;

use bevy::{
    feathers::{containers::flex_spacer, cursor::EntityCursor},
    prelude::*,
    ui_widgets::{Activate, Button},
};
use file_tagger_internals::{DatabaseState, SearchMenu, SearchState};

use crate::{
    state::LensState,
    widgets::{button, label, submittable_text_input, task},
};

use super::centered_box;
use super::launcher::launcher;

pub fn search_menu_view(mut state: LensState<SearchMenu>) -> impl Scene + use<> {
    centered_box(bsn_list![
        active_folder_name(&state.database)
        --
        search_bar(state.deref_mut())
        --
        edit_buttons(state.deref_mut())
        --
        launcher(state.deref_mut())
        --
        flex_spacer()
    ])
}

pub fn active_folder_name(state: &DatabaseState) -> impl Scene + use<> {
    bsn! {
        label(state.active_folder().name.to_string_lossy().into_owned())
        TextFont {
            font_size: FontSize::Px(20.0),
            weight: FontWeight::BOLD
        }
    }
}

pub fn search_bar<S>(state: &mut S) -> impl Scene + use<S>
where
    S: SearchState,
    for<'a> LensState<'a, S>: DerefMut<Target = S>,
{
    submittable_text_input(
        |mut state: LensState<S>| {
            state.deref_mut().search_results();
        },
        bsn! {
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::top(px(6)),
            }
            Button
            EntityCursor::System(bevy::window::SystemCursorIcon::Pointer)
            Children [
                label("🔍")
            ]
        },
    )
}

pub fn edit_buttons<S>(state: &mut S) -> impl Scene + use<S>
where
    S: SearchState,
    for<'a> LensState<'a, S>: DerefMut<Target = S>,
{
    bsn! {
        Node {
            justify_content: JustifyContent::Center,
        }
        Children [
            button(bsn!(label("Edit Entries")))
            on(
                |on: On<Activate>, mut state: LensState<S>| {
                    state.edit_entries();
                },
            )
            --
            button(bsn!(label("Import Files")))
            on(
                |on: On<Activate>, mut commands: Commands, mut state: LensState<S>| {
                    state.start_import_dialog();
                    commands.entity(on.entity).apply_scene(task(
                        file_tagger_internals::import_files,
                        |In(result): In<_>, mut state: LensState<S>| {
                            state.handle_import_dialog(result);
                        },
                    ));
                },
            )
        ]
    }
    // flex_row((
    //     text_button("Edit Entries", |state: &mut S| state.edit_entries()),
    //     fork(
    //         text_button("Import Files", |state: &mut S| {
    //             state.start_import_dialog();
    //         }),
    //         state.import_dialog_active().then_some(task(
    //             |proxy, _| async move {
    //                 let result = rfd::AsyncFileDialog::new()
    //                     .set_title("Select Files to Import")
    //                     .pick_files()
    //                     .await;
    //                 proxy.message(result);
    //             },
    //             |state: &mut S, result| {
    //                 state.stop_import_dialog();
    //                 if let Some(files) = result
    //                     && !files.is_empty()
    //                 {
    //                     state.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
    //                 }
    //             },
    //         )),
    //     ),
    // ))
    // .main_axis_alignment(MainAxisAlignment::Center)
}
