use bevy::{
    feathers::{FeathersPlugins, dark_theme::create_dark_theme, theme::UiTheme},
    prelude::*,
};

mod state;
mod view;

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, state::plugin, view::plugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .add_systems(Startup, spawn_camera)
        .run()
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
