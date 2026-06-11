use std::ops::{Deref, DerefMut};

use file_tagger_internals::{ActiveOverlay, ActiveView, AppState};
use miette::{IntoDiagnostic, MietteHandlerOpts};
use xilem::{
    EventLoop, ViewCtx, WidgetView, WindowOptions, Xilem,
    core::{AnyView, NoElement, View, one_of::Either},
    view::zstack,
    winit::error::EventLoopError,
};

use crate::{
    assets::Assets,
    edit::TransactionApi,
    view::{
        edit_view, error_view, launcher_view, search_menu_view, search_results_view, spinner_view,
    },
};

mod app_data;
mod assets;
mod edit;
mod launcher;
mod search_menu;
mod search_results;
mod view;

/// The `alongside_view` for `fork` requires the element type to be `NoElement`, but `AnyWidgetView` forces it to be `Pod<Passthrough>`
type AnyTaskView<State, Action = ()> = dyn AnyView<State, Action, ViewCtx, NoElement> + Send + Sync;

/// State wrapper so we can store xilem-specific state
struct XilemAppState {
    state: AppState,
    assets: Assets,
    /// Displayed by [`crate::view::spinner_view`] when the active overlay is [`ActiveOverlay::Spinner`]
    pending_task: Option<Box<dyn Fn(&mut AppState) -> Box<AnyTaskView<AppState>>>>,
    active_transaction: TransactionApi,
}

impl Deref for XilemAppState {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for XilemAppState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl XilemAppState {
    fn new() -> miette::Result<Self> {
        let state = AppState::new()?;
        let assets = Assets::new().into_diagnostic()?;
        Ok(Self {
            state,
            assets,
            pending_task: None,
            active_transaction: TransactionApi::new_disconnected(),
        })
    }

    fn app_logic(&mut self) -> impl WidgetView<XilemAppState> + use<> {
        let view = match self.active_view {
            ActiveView::Launcher => launcher_view(self).boxed(),
            ActiveView::SearchMenu => search_menu_view(self).boxed(),
            ActiveView::SearchResults => search_results_view(self).boxed(),
            ActiveView::Edit => edit_view(self).boxed(),
        };

        if let Some(overlay) = match &self.active_overlay {
            ActiveOverlay::None => None,
            ActiveOverlay::Error(report) => Some(
                error_view(report, |state: &mut XilemAppState| {
                    state.active_overlay = ActiveOverlay::None;
                })
                .boxed(),
            ),
            ActiveOverlay::Spinner => Some(spinner_view(self).boxed()),
        } {
            Either::A(zstack((view, overlay)))
        } else {
            Either::B(view)
        }
    }

    fn run_task<V>(&mut self, view: impl Fn(&mut AppState) -> V + 'static)
    where
        V: View<AppState, (), ViewCtx, Element = NoElement> + Send + Sync,
    {
        self.active_overlay = ActiveOverlay::Spinner;
        self.pending_task = Some(Box::new(move |state| Box::new(view(state))));
    }

    fn run_app(self) -> Result<(), EventLoopError> {
        Xilem::new_simple(self, Self::app_logic, WindowOptions::new("File Tagger"))
            .run_in(EventLoop::with_user_event())
    }
}

struct FuckedState(miette::Report);

impl FuckedState {
    fn app_logic(&mut self) -> impl WidgetView<Self> + use<> {
        error_view(&self.0, |_| std::process::exit(1))
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
    XilemAppState::new()
        .map_or_else(FuckedState::run_app, XilemAppState::run_app)
        .into_diagnostic()
}
