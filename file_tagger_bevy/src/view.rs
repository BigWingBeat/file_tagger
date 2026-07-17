use bevy::{
    color::palettes::tailwind::{ZINC_600, ZINC_700, ZINC_800, ZINC_900},
    prelude::*,
};

use file_tagger_internals::UnrecoverableError;

use crate::state::{LensState, State};

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
                width: px(1000.0),
                height: px(375.0),
                padding: px(10.0)
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
        // BackgroundColor(ZINC_900)
        Children [
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: px(3.0),
                border_radius: px(6.0),
                padding: px(16.0)
            }
            BorderColor::from(ZINC_600)
            BackgroundColor(ZINC_700)
            Children [ {seq} ]
        ]
    }
}

// pub fn error_view<State, E, F>(e: &E, callback: F) -> impl WidgetView<State> + use<State, E, F>
// where
//     F: Fn(&mut State) + Send + Sync + 'static,
//     E: Debug + 'static,
//     State: 'static,
// {
//     centered_flex_box((
//         prose(format!("{e:?}"))
//             .font(GenericFamily::Monospace)
//             .weight(FontWeight::BOLD)
//             .text_size(20.0)
//             .text_color(RED),
//         text_button("Oops", callback),
//     ))
// }

pub fn error_view(state: LensState<UnrecoverableError>) -> impl Scene + use<> {
    centered_flex_box(bsn_list![Text("Text1"), Text("Text2")])
}
