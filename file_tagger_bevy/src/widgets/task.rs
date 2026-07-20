use std::marker::PhantomData;

use bevy::{
    ecs::{system::SystemId, template::TemplateContext},
    prelude::*,
    scene::{ResolveContext, ResolveSceneError, ResolvedScene},
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

#[derive(Component, Deref, DerefMut)]
pub struct DynTask {
    task: Box<dyn TaskApi + Send + Sync>,
}

fn handle_tasks(mut commands: Commands, tasks: Query<(Entity, &mut DynTask)>) {
    for (entity, mut task) in tasks {
        if task.handle_if_finished(&mut commands) {
            commands.entity(entity).remove::<DynTask>();
        }
    }
}

pub struct TaskTemplate<T, S, I, M> {
    task: T,
    system: S,
    marker: PhantomData<(I, M)>,
}

impl<T, S, I, M> TaskTemplate<T, S, I, M> {
    pub fn new(task: T, system: S) -> Self {
        Self {
            task,
            system,
            marker: PhantomData,
        }
    }
}

impl<T: Clone, S: Clone, I, M> Clone for TaskTemplate<T, S, I, M> {
    fn clone(&self) -> Self {
        Self {
            task: self.task.clone(),
            system: self.system.clone(),
            marker: PhantomData,
        }
    }
}

impl<T, F, S, I, M> Template for TaskTemplate<T, S, I, M>
where
    T: Fn() -> F + Clone,
    F: Future<Output = I::Inner<'static>> + Send + 'static,
    S: IntoSystem<I, (), M> + Clone + 'static,
    I: SystemInput<Inner<'static>: Send> + 'static,
{
    type Output = DynTask;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let task_pool = IoTaskPool::try_get().ok_or("IoTaskPool is not initialized yet")?;
        let task = task_pool.spawn((self.task)());

        let system = context
            .entity
            .world_scope(|world| world.register_system_cached(self.system.clone()));

        Ok(DynTask {
            task: Box::new(Task { task, system }),
        })
    }

    fn clone_template(&self) -> Self {
        self.clone()
    }
}

impl<T, S, I, M> Scene for TaskTemplate<T, S, I, M>
where
    Self: Template<Output: Component> + Send + Sync + 'static,
{
    fn resolve(
        self,
        _context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        scene.push_template(self);
        Ok(())
    }
}

/// These bounds aren't strictly needed, as they are just duplicated from the `Template` impl,
/// but having them here too results in better error messages when they are unsatisfied
pub fn task<T, F, S, I, M>(task: T, system: S) -> TaskTemplate<T, S, I, M>
where
    T: Fn() -> F + Clone,
    F: Future<Output = I::Inner<'static>> + Send + 'static,
    S: IntoSystem<I, (), M> + Clone + 'static,
    I: SystemInput<Inner<'static>: Send> + 'static,
{
    TaskTemplate::new(task, system)
}
