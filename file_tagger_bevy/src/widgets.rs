use bevy::{
    color::palettes::tailwind::ZINC_700,
    feathers::cursor::EntityCursor,
    prelude::*,
    text::{FontSourceTemplate, LetterSpacing},
    ui_widgets::Button,
};

mod task;

pub use task::{DynTask, Task, TaskApi, TaskTemplate, task};

pub fn plugin(app: &mut App) {
    app.add_plugins(task::plugin);
    // app.add_systems(Update, button_system);
}

pub fn label(text: impl Into<String>) -> impl Scene {
    bsn! {
        Text(text)
        TextFont {
            font: FontSourceTemplate::SystemUi,
            font_size: px(14.0),
        }
        // Bevy has tighter kerning than Xilem, this brings it roughly back in line
        template_value(LetterSpacing::Px(0.5))
    }
}

pub fn button(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(16.0), px(6.0)),
            border: px(1.0),
            border_radius: px(6.0),
        }
        Button
        BorderColor::all(ZINC_700)
        EntityCursor::System(bevy::window::SystemCursorIcon::Pointer)
        Children [
            {children}
        ]
    }
}
