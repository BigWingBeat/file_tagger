use bevy::{
    color::palettes::{
        css::RED,
        tailwind::{ZINC_600, ZINC_700},
    },
    feathers::cursor::EntityCursor,
    input::keyboard::{Key, KeyboardInput},
    input_focus::{AutoFocus, FocusedInput, tab_navigation::TabIndex},
    prelude::*,
    text::{EditableText, FontSourceTemplate, LetterSpacing, TextCursorStyle},
    ui_widgets::{Activate, Button},
    window::SystemCursorIcon,
};

mod task;

pub use task::{DynTask, Task, TaskApi, task};

pub const TEXT_COLOUR: Srgba = Srgba::rgb(0.9490196, 0.9490196, 0.9490196);
pub const TEXT_INPUT_BG: Srgba = Srgba::rgb(0.08627451, 0.08627451, 0.08627451);

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
        TextColor(TEXT_COLOUR)
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

pub fn text_input<C, M>(callback: C) -> impl Scene
where
    C: IntoSystem<(), (), M> + Clone + Send + Sync + 'static,
    M: 'static,
{
    bsn! {
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(12), px(6)),
            border: UiRect::all(px(1)),
            width: Val::Percent(100.0),
            border_radius: BorderRadius::all(px(4)),
            column_gap: px(4),
        }
        BorderColor::all(ZINC_600)
        BackgroundColor(TEXT_INPUT_BG)
        Children [
            Node {
                width: Val::Percent(100.0),
            }
            EditableText {
                cursor_width: 0.1,
            }
            TextLayout {
                linebreak: LineBreak::NoWrap,
            }
            AutoFocus
            TabIndex(0)
            TextFont {
                font: FontSourceTemplate::SystemUi,
                font_size: px(14),
            }
            TextColor(TEXT_COLOUR)
            template_value(LetterSpacing::Px(0.5))
            EntityCursor::System(SystemCursorIcon::Text)
            TextCursorStyle {
                color: TEXT_COLOUR,
            }
            on(move |on: On<FocusedInput<KeyboardInput>>, mut commands: Commands| {
                if on.input.state.is_pressed() && on.input.logical_key == Key::Enter {
                    commands.run_system_cached(callback.clone());
                }
            })
        ]
    }
}

pub fn text_input_with<C, M>(callback: C, children: impl SceneList) -> impl Scene
where
    C: IntoSystem<(), (), M> + Clone + Send + Sync + 'static,
    M: 'static,
{
    bsn! {
        text_input(callback)
        Children [{children}]
    }
}

pub fn submittable_text_input<C, M>(callback: C, button: impl Scene) -> impl Scene
where
    C: IntoSystem<(), (), M> + Clone + Send + Sync + 'static,
    M: 'static,
{
    bsn! {
        text_input(callback.clone())
        Children [
            (
                {button}
                on(move |_: On<Activate>, mut commands: Commands| {
                    commands.run_system_cached(callback.clone());
                })
            ),
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
