use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy::{ecs::system::SystemParam, prelude::*};
use file_tagger_internals::{
    ActiveView, Edit, Launcher, SearchMenu, SearchResults, UnrecoverableError,
};

pub fn plugin(app: &mut App) {
    app.insert_non_send(ActiveView::new(()))
        .init_state::<AppState>()
        .add_systems(PostUpdate, update_app_state);
}

pub trait State {
    const STATE: AppState;
}

#[derive(SystemParam)]
pub struct LensState<'w, T: 'static> {
    state: NonSendMut<'w, ActiveView>,
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
            let view_changed = state.update_to_next(|_, _, _| Box::new(()));
            if !view_changed {
                return;
            }

            match *state {
                $(ActiveView::$variant(_) => (*next_state).set_if_neq(AppState::$variant)),*
            }
        }

        $(
            impl State for $variant { const STATE: AppState = AppState::$variant; }

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
