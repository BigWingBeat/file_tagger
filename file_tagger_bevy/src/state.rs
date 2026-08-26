use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy::{ecs::system::SystemParam, prelude::*};
use file_tagger_internals::{
    ActiveOverlay, ActiveView, Edit, Launcher, SearchMenu, SearchResults, UnrecoverableError,
};
use miette::Report;

pub fn plugin(app: &mut App) {
    app.insert_non_send(ActiveView::new(()))
        .init_state::<AppState>()
        .init_state::<OverlayState>()
        .add_systems(PostUpdate, (update_app_state, update_overlay_state).chain());
}

pub trait State {
    type State: States;
    const STATE: Self::State;
}

#[derive(SystemParam)]
pub struct LensState<'w, T: 'static> {
    pub state: NonSendMut<'w, ActiveView>,
    __: PhantomData<T>,
}

macro_rules! app_state {
    ($($variant:ident),* $(,)*) => {
        #[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum AppState {
            $($variant),*
        }

        impl FromWorld for AppState {
            fn from_world(world: &mut World) -> Self {
                match world.non_send::<ActiveView>() {
                    $(ActiveView::$variant(_) => Self::$variant),*
                }
            }
        }

        fn update_app_state(mut state: NonSendMut<ActiveView>, mut next_state: ResMut<NextState<AppState>>) {
            let view_changed = state.update_to_next(|_, _, data| data);
            if !view_changed {
                return;
            }

            match *state {
                $(ActiveView::$variant(_) => next_state.as_mut().set_if_neq(AppState::$variant)),*
            }
        }

        $(
            impl State for $variant { type State = AppState; const STATE: Self::State = AppState::$variant; }

            impl Deref for LensState<'_, $variant> {
                type Target = $variant;

                fn deref(&self) -> &Self::Target {
                    match self.state.deref() {
                        ActiveView::$variant(inner) => inner,
                        // The alternative to this is to not use `ActiveView` and instead juggle the different inner
                        // state types in individual resources, which would be much worse DX, and still fallible
                        _ => unreachable!("State was changed unexpectedly"),
                    }
                }
            }

            impl DerefMut for LensState<'_, $variant> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    match self.state.deref_mut() {
                        ActiveView::$variant(inner) => inner,
                        _ => unreachable!("State was changed unexpectedly"),
                    }
                }
            }
        )*
    };
}

app_state!(
    UnrecoverableError,
    Launcher,
    SearchMenu,
    SearchResults,
    Edit,
);

impl Deref for LensState<'_, Report> {
    type Target = Report;

    fn deref(&self) -> &Self::Target {
        let ActiveOverlay::Error(report) = self.state.active_overlay() else {
            unreachable!("State was changed unexpectedly");
        };
        report
    }
}

impl State for Report {
    type State = OverlayState;
    const STATE: Self::State = OverlayState::Error;
}

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OverlayState {
    None,
    Error,
    Spinner,
}

impl From<&ActiveView> for OverlayState {
    fn from(state: &ActiveView) -> Self {
        match state.active_overlay() {
            ActiveOverlay::None => Self::None,
            ActiveOverlay::Error(_) => Self::Error,
            ActiveOverlay::Spinner => Self::Spinner,
        }
    }
}

impl FromWorld for OverlayState {
    fn from_world(world: &mut World) -> Self {
        world.non_send::<ActiveView>().into()
    }
}

fn update_overlay_state(
    state: NonSend<ActiveView>,
    mut next_state: ResMut<NextState<OverlayState>>,
) {
    if !state.is_changed() {
        return;
    }

    next_state.as_mut().set_if_neq(state.as_ref().into());
}

/// The spinner overlay has no associated state, so we use this dummy type instead
pub struct Spinner;

impl State for Spinner {
    type State = OverlayState;
    const STATE: Self::State = OverlayState::Spinner;
}
