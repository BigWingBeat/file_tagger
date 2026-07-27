use bevy::{
    ecs::{
        system::SystemId,
        template::{FnTemplate, TemplateContext},
    },
    prelude::*,
    tasks::{IoTaskPool, futures::check_ready},
};

pub fn plugin(app: &mut App) {
    app.add_systems(Update, handle_tasks);
}

pub trait TaskApi {
    fn handle_if_finished(&mut self, commands: &mut Commands) -> bool;
}

pub struct Task<T: SystemInput> {
    task: bevy::tasks::Task<T::Inner<'static>>,
    system: SystemId<T>,
}

impl<T> TaskApi for Task<T>
where
    T: SystemInput<Inner<'static>: Send> + 'static,
{
    fn handle_if_finished(&mut self, commands: &mut Commands) -> bool {
        check_ready(&mut self.task).is_some_and(|result| {
            commands.run_system_with(self.system, result);
            true
        })
    }
}

/// `Task` is a generic type, so to be able to handle every monomorphization of it in one system, we hide it behind a trait object.
/// This type can't impl `FromTemplate`, but can still be used in `bsn` via the `task` helper method.
/// That is because `FromTemplate` requires specifying a single (monomorphized) template type, but we need a generic template type, to
/// provide the underlying generic `Task` type instance.
#[derive(Component)]
pub struct DynTask(Box<dyn TaskApi + Send + Sync>);

fn handle_tasks(mut commands: Commands, tasks: Query<(Entity, &mut DynTask)>) {
    for (entity, mut task) in tasks {
        if task.0.handle_if_finished(&mut commands) {
            commands.entity(entity).remove::<DynTask>();
        }
    }
}

/// The aforementioned helper method. Works a lot like Xilem's `task` view fn:
/// The first parameter is a fn that returns a future,
/// and the second parameter is a Bevy system that takes the future's output value as an `In<T>` input
pub fn task<T, F, S, I, M>(
    task_fn: T,
    system: S,
) -> FnTemplate<impl Fn(&mut TemplateContext<'_, '_>) -> Result<DynTask> + Clone, DynTask>
where
    T: Fn() -> F + Clone + 'static,
    F: Future<Output = I::Inner<'static>> + Send + 'static,
    S: IntoSystem<I, (), M> + Clone + 'static,
    I: SystemInput<Inner<'static>: Send> + 'static,
{
    template(move |context| {
        let task_pool = IoTaskPool::try_get().ok_or("IoTaskPool must be initialized")?;
        let task = task_pool.spawn((task_fn)());

        // Use `register_system_cached` to avoid a memory leak from duplicating system registrations
        // when the same task is despawned and respawned (as opposed to `register_system`)
        let system = context
            .entity
            .world_scope(|world| world.register_system_cached(system.clone()));

        Ok(DynTask(Box::new(Task { task, system })))
    })
}
