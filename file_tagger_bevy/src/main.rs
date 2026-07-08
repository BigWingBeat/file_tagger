use bevy::{
    feathers::{FeathersPlugins, dark_theme::create_dark_theme, theme::UiTheme},
    prelude::*,
};
use file_tagger_internals::ActiveView;

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_non_send(ActiveView::new(()))
        .add_systems(Startup, scene.spawn())
        .run()
}

fn scene() -> impl SceneList {
    bsn_list![Camera2d, ui()]
}

fn ui() -> impl Scene {
    bsn! {
        Node {}
        Children [
            AccessibleLabel("Text")
        ]
    }
}
