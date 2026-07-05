use miette::{IntoDiagnostic, MietteHandlerOpts};
use xilem::{
    EventLoop, ViewCtx, WindowOptions, Xilem,
    core::{AnyView, NoElement},
};

use crate::state::{app_logic, init_state};

mod assets;
mod state;
mod view;

const WINDOW_TITLE: &str = "File Tagger";

/// The `alongside_view` for `fork` requires the element type to be `NoElement`, but `AnyWidgetView` forces it to be `Pod<Passthrough>`
type AnyTaskView<State, Action = ()> = dyn AnyView<State, Action, ViewCtx, NoElement> + Send + Sync;

fn set_error_handler() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            MietteHandlerOpts::new()
                .terminal_links(false)
                .width(80)
                .wrap_lines(true)
                .break_words(true)
                .with_cause_chain()
                // .without_cause_chain()
                .show_related_errors_as_nested()
                // .show_related_errors_as_siblings()
                .color(false)
                .unicode(true)
                .force_graphical(true)
                .context_lines(1)
                .build(),
        )
    }))
    .unwrap();
}

fn main() -> miette::Result<()> {
    set_error_handler();
    Xilem::new_simple(init_state(), app_logic, WindowOptions::new(WINDOW_TITLE))
        .run_in(EventLoop::with_user_event())
        .into_diagnostic()
}
