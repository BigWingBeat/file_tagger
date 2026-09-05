use std::ops::DerefMut;

use bevy::{prelude::*, ui_widgets::Activate};
use file_tagger_internals::{Launcher, LauncherState};

use crate::{
    state::LensState,
    widgets::{button, label, task},
};

use super::centered_box;

pub fn launcher_view(mut state: LensState<Launcher>) -> impl Scene + use<> {
    centered_box(bsn_list![launcher(state.deref_mut())])
}

pub fn launcher<L>(state: &mut L) -> impl Scene + use<L>
where
    L: LauncherState,
    for<'a> LensState<'a, L>: DerefMut<Target = L>,
{
    bsn! {
        Node {
            column_gap: px(10.0),
        }
        Children [
            recent_list_portal(state)
            Node { width: Val::Percent(100.0) }
            --
            open_create_buttons(state)
            Node { width: Val::Percent(100.0) }
        ]
    }
}

fn recent_list_portal<L>(state: &mut L) -> impl Scene + use<L>
where
    L: LauncherState,
    for<'a> LensState<'a, L>: DerefMut<Target = L>,
{
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10.0),
        }
        Children [
            label("Open Recent")
            TextFont {
                font_size: FontSize::Px(20.0),
                weight: FontWeight::BOLD
            }
            --
            Node {
                overflow: Overflow::scroll_y()
            }
            Children [
                recent_list(state)
            ]
        ]
    }
}

fn recent_list<L>(state: &mut L) -> impl Scene + use<L>
where
    L: LauncherState,
    for<'a> LensState<'a, L>: DerefMut<Target = L>,
{
    bsn! {
        Node {
            flex_direction: FlexDirection::ColumnReverse,
            row_gap: px(1.0),
        }
        Children [
            {
                state
                .persistent()
                .recent_folders()
                .iter()
                .map(|folder| {
                    let path = folder.path.clone();
                    bsn! {
                        button(
                            bsn! {
                                Node {
                                    flex_direction: FlexDirection::Column,
                                }
                                Children [
                                    label(folder.name.to_string_lossy().into_owned())
                                    TextFont {
                                        weight: FontWeight::BOLD
                                    }
                                    --
                                    label(path.to_string_lossy().into_owned())
                                ]
                            }
                        )
                        Node {
                            justify_content: JustifyContent::Start,
                            border: px(0.0)
                        }
                        on(move |_: On<Activate>, mut state: LensState<L>| {
                            state.open_database_in_folder(path.clone());
                        })
                    }
                })
                .collect::<Vec<_>>()
            }
        ]
    }
}

fn open_create_buttons<L>(state: &mut L) -> impl Scene + use<L>
where
    L: LauncherState,
    for<'a> LensState<'a, L>: DerefMut<Target = L>,
{
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceEvenly,
            row_gap: px(10.0)
        }
        Children [
            Node {
                column_gap: px(10.0),
            }
            Children [
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Start,
                    width: Val::Percent({200.0 / 3.0}),
                }
                Children [
                    label("Open Folder As Database")
                    TextFont {
                        weight: FontWeight::BOLD
                    }
                    --
                    label("Open or create a database in a folder")
                ]
                --
                button(bsn!(label("Open")))
                Node {
                    height: Val::Percent(100.0),
                    width: Val::Percent({100.0 / 3.0}),
                }
                on(
                    |on: On<Activate>, mut commands: Commands, mut state: LensState<L>| {
                        state.start_open_dialog();
                        commands.entity(on.entity).apply_scene(task(
                            file_tagger_internals::open_folder_as_db,
                            |In(result): In<_>, mut state: LensState<L>| {
                                state.handle_open_dialog(result);
                            },
                        ));
                    },
                )
            ]
            --
            Node {
                column_gap: px(10.0),
            }
            Children [
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Start,
                    width: Val::Percent({200.0 / 3.0}),
                }
                Children [
                    label("Create New Database")
                    TextFont {
                        weight: FontWeight::BOLD
                    }
                    --
                    label("Create a new folder with a new database")
                ]
                --
                button(bsn!(label("Create")))
                Node {
                    height: Val::Percent(100.0),
                    width: Val::Percent({100.0 / 3.0}),
                }
                on(
                    |on: On<Activate>, mut commands: Commands, mut state: LensState<L>| {
                        state.start_create_dialog();
                        commands.entity(on.entity).apply_scene(task(
                            file_tagger_internals::create_folder_and_db,
                            |In(result): In<_>, mut state: LensState<L>| {
                                state.handle_create_dialog(result);
                            },
                        ));
                    },
                )
            ]
        ]
    }
}
