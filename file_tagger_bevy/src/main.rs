use bevy::{feathers::FeathersPlugins, prelude::*};

mod state;
mod view;
mod widgets;

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, state::plugin, view::plugin))
        .add_systems(Startup, spawn_camera)
        .run()
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
