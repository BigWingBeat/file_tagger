use bevy::{
    color::palettes::{css::RED, tailwind::ZINC_700},
    feathers::cursor::EntityCursor,
    prelude::*,
    text::{FontSourceTemplate, LetterSpacing},
    ui_widgets::Button,
};

mod task;

pub use task::{DynTask, Task, TaskApi, task};

pub fn plugin(app: &mut App) {
    app.add_plugins(task::plugin);
    // .add_systems(Update, button_system);
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

fn button_system(
    // mut input_focus: ResMut<InputFocus>,
    mut interaction_query: Query<
        (
            Entity,
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Button,
            &Children,
        ),
        Changed<Interaction>,
    >,
) {
    const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
    const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
    const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);

    for (entity, interaction, mut color, mut border_color, mut button, children) in
        &mut interaction_query
    {
        match *interaction {
            Interaction::Pressed => {
                // input_focus.set(entity, FocusCause::Pressed);
                *color = PRESSED_BUTTON.into();
                *border_color = BorderColor::all(RED);

                // The accessibility system's only update the button's state when the `Button` component is marked as changed.
                button.set_changed();
            }
            Interaction::Hovered => {
                // input_focus.set(entity, FocusCause::Pressed);
                *color = HOVERED_BUTTON.into();
                *border_color = BorderColor::all(Color::WHITE);
                button.set_changed();
            }
            Interaction::None => {
                // input_focus.clear();
                *color = NORMAL_BUTTON.into();
                *border_color = BorderColor::all(Color::BLACK);
            }
        }
    }
}
