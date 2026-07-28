use std::ops::Deref;

use bevy::{
    color::palettes::{
        css::RED,
        tailwind::{ZINC_600, ZINC_700, ZINC_800, ZINC_900},
    },
    prelude::*,
    text::FontSourceTemplate,
    ui_widgets::Activate,
};

use file_tagger_internals::{ActiveOverlay, ActiveView, UnrecoverableError};
use miette::Report;

use crate::{
    state::{LensState, Spinner, State},
    widgets::{button, label},
};

mod edit;
mod launcher;
mod search_menu;
mod search_results;

use edit::edit_view;
use launcher::launcher_view;
use search_menu::search_menu_view;
use search_results::search_results_view;

fn add_view_system<S, F, Out, Marker>(app: &mut App, f: F)
where
    S: State + 'static,
    Out: Scene,
    F: IntoSystem<(), Out, Marker> + for<'a> Fn(LensState<'a, S>) -> Out,
{
    app.add_systems(OnEnter(S::STATE), f.pipe(spawn_view::<S, Out>));
}

fn spawn_view<S: State, T: Scene>(In(view): In<T>, mut commands: Commands) {
    commands.spawn(DespawnOnExit(S::STATE)).apply_scene(view);
}

pub fn plugin(app: &mut App) {
    add_view_system(app, error_view);
    add_view_system(app, launcher_view);
    add_view_system(app, search_menu_view);
    add_view_system(app, search_results_view);
    add_view_system(app, edit_view);

    add_view_system(app, spinner_overlay);
    add_view_system(app, error_overlay);
}

fn centered_box(seq: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            width: percent(100),
            height: percent(100),
        }
        BackgroundColor(ZINC_900)
        Children [
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                width: px(1000),
                height: px(375),
                padding: px(10),
                row_gap: px(10),
            }
            BackgroundColor(ZINC_800)
            Children [ {seq} ]
        ]
    }
}

fn centered_flex_box(seq: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            width: percent(100),
            height: percent(100),
        }
        Children [
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: px(3),
                border_radius: px(6),
                padding: px(16)
            }
            BorderColor::from(ZINC_600)
            BackgroundColor(ZINC_700)
            Children [ {seq} ]
        ]
    }
}

/// This state should always have an active error overlay
pub fn error_view(state: LensState<UnrecoverableError>) -> impl Scene + use<> {}

// Overlay views for `ActiveOverlay` states

fn spinner_overlay(state: LensState<Spinner>) -> impl Scene + use<> {
    bsn!(centered_flex_box(bsn_list![Text("TODO: Insert Spinner Here"), Text("Waiting for dialog...")]) ZIndex(1))
}

fn error_overlay(state: LensState<Report>) -> impl Scene + use<> {
    let is_unrecoverable = matches!(state.state.deref(), ActiveView::UnrecoverableError(_));
    let e = state.deref();

    bsn!(centered_flex_box(bsn_list![
        (
            label(format!("{e:?}"))
            TextFont {
                font: FontSourceTemplate::Monospace,
                font_size: px(20),
                weight: FontWeight::BOLD,
            }
            TextColor(RED)
        ),
        (
            button(bsn!(label("Oops")))
            on(move |_: On<Activate>, mut state: NonSendMut<ActiveView>, mut exit: MessageWriter<AppExit>| {
                if is_unrecoverable {
                    exit.write(AppExit::from_code(1));
                } else {
                    state.set_active_overlay(ActiveOverlay::None);
                }
            })
        ),
    ]) ZIndex(1))
}
