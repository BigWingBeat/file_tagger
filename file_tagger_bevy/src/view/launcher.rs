use std::ops::DerefMut;

use bevy::prelude::*;
use file_tagger_internals::{Launcher, LauncherState};

use crate::{
    state::LensState,
    widgets::{button, label},
};

use super::centered_box;

pub fn launcher_view(mut state: LensState<Launcher>) -> impl Scene + use<> {
    centered_box(bsn_list![launcher(state.deref_mut())])
}

pub fn launcher(state: &mut Launcher) -> impl Scene + use<> {
    bsn! {
        Node {
            column_gap: px(10.0)
        }
        Children [
            (
                recent_list_portal(state)
                Node { flex_grow: 0.5 }
            ),
            (
                open_create_buttons(state)
                Node { flex_grow: 0.5 }
            ),
        ]
    }
}

fn recent_list_portal(state: &mut Launcher) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10.0)
        }
        Children [
            (
                label("Open Recent")
                TextFont {
                    font_size: FontSize::Px(20.0),
                    weight: FontWeight::BOLD
                }
            ),
            (
                Node {
                    overflow: Overflow::scroll_y()
                }
                Children [
                    recent_list(state)
                ]
            )
        ]
    }
}

fn recent_list(state: &mut Launcher) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            row_gap: px(1.0),
        }
        Children [
            {
                state
                .persistent()
                .recent_folders()
                .iter()
                .rev()
                .map(|folder| {
                    bsn! {
                        button(
                            bsn! {
                                Node {
                                    flex_direction: FlexDirection::Column,
                                }
                                Children [
                                    (
                                        label(folder.name.to_string_lossy().into_owned())
                                        TextFont {
                                            weight: FontWeight::BOLD
                                        }
                                    ),
                                    label(folder.path.to_string_lossy().into_owned())
                                ]
                            }
                        )
                        Node {
                            justify_content: JustifyContent::Start,
                            border: px(0.0)
                        }
                    }
                })
                .collect::<Vec<_>>()
            }
        ]
    }
}

fn open_create_buttons(state: &mut Launcher) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceEvenly,
            row_gap: px(10.0)
        }
        Children [
            (
                Node {
                    column_gap: px(10.0)
                }
                Children [
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Start,
                            flex_grow: { 2.0 / 3.0 },
                        }
                        Children [
                            (
                                label("Open Folder As Database")
                                TextFont {
                                    weight: FontWeight::BOLD
                                }
                            ),
                            label("Open or create a database in a folder")
                        ]
                    ),
                    (
                        button(bsn!(label("Open")))
                        Node {
                            height: Val::Percent(100.0),
                            flex_grow: { 1.0 / 3.0 },
                        }
                    )
                ]
            ),
            (
                Node
                Children [
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Start,
                            flex_grow: { 2.0 / 3.0 },
                        }
                        Children [
                            (
                                label("Create New Database")
                                TextFont {
                                    weight: FontWeight::BOLD
                                }
                            ),
                            label("Create a new folder with a new database")
                        ]
                    ),
                    (
                        button(bsn!(label("Create")))
                        Node {
                            height: Val::Percent(100.0),
                            flex_grow: { 1.0 / 3.0 },
                        }
                    )
                ]
            ),
        ]
    }
}
