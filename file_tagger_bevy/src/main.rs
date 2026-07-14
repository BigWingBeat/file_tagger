use bevy::{
    feathers::{FeathersPlugins, dark_theme::create_dark_theme, theme::UiTheme},
    prelude::*,
};

mod state;
mod view;

fn main() -> AppExit {
    let mut theme = UiTheme(create_dark_theme());
    theme.0.color.insert(
        bevy::feathers::tokens::TEXT_MAIN,
        bevy::feathers::palette::WHITE,
    );
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, state::plugin, view::plugin))
        .insert_resource(theme)
        .add_systems(Startup, spawn_camera)
        .run()
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
