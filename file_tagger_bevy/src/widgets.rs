use bevy::{color::palettes::tailwind::ZINC_700, prelude::*, text::FontSourceTemplate};

pub fn label(text: impl Into<String>) -> impl Scene {
    bsn! {
        Text(text)
        TextFont {
            font: FontSourceTemplate::SystemUi,
            font_size: px(14.0),
        }
    }
}

pub fn button(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            // height: size::ROW_HEIGHT,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(16.0), px(6.0)),
            border: px(1.0),
            border_radius: px(6.0),
        }
        Button
        BorderColor::all(ZINC_700)
        // Hovered
        // CursorIcon::System(bevy::window::SystemCursorIcon::Pointer)
        // TabIndex(0)
        Children [
            {children}
        ]
    }
}
