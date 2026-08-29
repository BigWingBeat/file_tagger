use bevy::{feathers::FeathersPlugins, prelude::*};
// use bevy_inspector_egui::bevy_egui::EguiPlugin;

mod state;
mod view;
mod widgets;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FeathersPlugins,
            state::plugin,
            view::plugin,
            widgets::plugin,
        ))
        // .add_plugins(EguiPlugin::default())
        // .add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new())
        .add_systems(Startup, spawn_camera)
        .run()
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
