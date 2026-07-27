use std::ops::DerefMut;

use bevy::{
    feathers::{
        containers::flex_spacer,
        controls::{FeathersTextInput, FeathersTextInputContainer},
    },
    prelude::*,
    ui_widgets::Activate,
};
use file_tagger_internals::{DatabaseState, SearchMenu, SearchState};
use rfd::FileHandle;

use crate::{
    state::LensState,
    widgets::{button, label, task},
};

use super::centered_box;
use super::launcher::launcher;

pub fn search_menu_view(mut state: LensState<SearchMenu>) -> impl Scene + use<> {
    centered_box(bsn_list![
        active_folder_name(&mut state.database),
        search_bar(&mut *state),
        edit_buttons(&mut *state),
        launcher(&mut *state),
        flex_spacer(),
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
    // prose(state.active_folder().name.to_string_lossy())
    //     .weight(FontWeight::BOLD)
    //     .text_size(20.0)
    //     .dims(Dimensions::width(Dim::Stretch))
}

pub fn search_bar<S>(state: &mut S) -> impl Scene + use<S>
where
    S: SearchState,
    for<'a> LensState<'a, S>: DerefMut<Target = S>,
{
    bsn! {
        @FeathersTextInputContainer
        Children [
            @FeathersTextInput,
            (
                button(bsn!(label("🔍")))
                on(
                    |on: On<Activate>, mut state: LensState<S>| {
                        state.deref_mut().search_results();
                    },
                )
            ),
        ]
    }
    // submittable_text_input(
    //     text_input(state.search_bar().clone(), |state: &mut S, text| {
    //         *state.search_bar() = text
    //     })
    //     .on_enter(|state: &mut S, _| state.search_results())
    //     .placeholder("Search database by tag"),
    //     text_button("🔍", |state: &mut S| state.search_results()),
    //     Dimensions::AUTO,
    // )
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
            (
                button(bsn!(label("Edit Entries")))
                on(
                    |on: On<Activate>, mut state: LensState<S>| {
                        state.edit_entries();
                    },
                )
            ),
            (
                button(bsn!(label("Import Files")))
                on(
                    |on: On<Activate>, mut commands: Commands, mut state: LensState<S>| {
                        state.start_import_dialog();
                        commands.entity(on.entity).apply_scene(task(
                            || async move {
                                rfd::AsyncFileDialog::new()
                                    .set_title("Select Files to Import")
                                    .pick_files()
                                    .await
                            },
                            |In(result): In<Option<Vec<FileHandle>>>, mut state: LensState<S>| {
                                state.stop_import_dialog();
                                if let Some(files) = result
                                    && !files.is_empty()
                                {
                                    state.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
                                }
                            },
                        ));
                    },
                )
            ),
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
