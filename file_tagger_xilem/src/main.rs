use file_tagger_internals::{ActiveView, AppState};
use miette::{IntoDiagnostic, MietteHandlerOpts};
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem, winit::error::EventLoopError};

use crate::view::{edit_view, launcher_view, overlay_error, search_menu_view, search_results_view};

mod app_data;
mod edit;
mod launcher;
mod search_menu;
mod search_results;
mod view;

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let view = match state.active_view {
        ActiveView::Launcher => launcher_view(state).boxed(),
        ActiveView::SearchMenu => search_menu_view(state).boxed(),
        ActiveView::SearchResults => search_results_view(state).boxed(),
        ActiveView::Edit => edit_view(state).boxed(),
    };
    let error = state.latest_error.as_ref();
    overlay_error(view, error, |state| {
        state.latest_error = None;
    })
}

fn run_app(state: AppState) -> Result<(), EventLoopError> {
    Xilem::new_simple(state, app_logic, WindowOptions::new("File Tagger"))
        .run_in(EventLoop::with_user_event())
}

struct FuckedState(miette::Report);

impl FuckedState {
    fn app_logic(&mut self) -> impl WidgetView<Self> + use<> {
        overlay_error((), Some(&self.0), |_| std::process::exit(1))
    }

    fn run_app(error: miette::Report) -> Result<(), EventLoopError> {
        Xilem::new_simple(
            Self(error),
            Self::app_logic,
            WindowOptions::new("File Tagger"),
        )
        .run_in(EventLoop::with_user_event())
    }
}

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
    AppState::new()
        .map_or_else(FuckedState::run_app, run_app)
        .into_diagnostic()
}
