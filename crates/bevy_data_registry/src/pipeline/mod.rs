//! The loading pipeline, step by step each frame: list and load every
//! file, wait for them, index each kind by id (families made whole),
//! validate across kinds, then finish: loaded, or failed and exiting.
//!
//! Once loaded, a file that changes (hot reload, with `dev`) is already
//! updated in place; every kind is then indexed and validated again,
//! problems logged, and [`DataReloaded`] sent. New files need a restart:
//! Bevy only reloads files it has loaded.

mod finish;
mod index;
mod load;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

pub use self::finish::DataReloaded;
use self::finish::Reload;
use self::load::{Files, Pending};
use crate::Definition;
use crate::registry::Registry;

/// Where loading the definitions is.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataState {
    /// Files are being listed and loaded.
    #[default]
    Loading,
    /// Every file is loaded, indexed and validated.
    Loaded,
    /// Something was wrong: logged, and the app told to exit.
    Failed,
}

/// Where your systems hook into loading.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataSystems {
    /// Checks across kinds, once every kind is indexed: when they've first
    /// loaded without errors, and after every hot reload. Push what's
    /// wrong to [`DataErrors`]: on first load, any stops the app; after a
    /// reload, they're logged.
    Validate,
}

/// What's wrong with the definitions: files that failed to load, files of
/// no kind, ids used twice, families that don't resolve, and what
/// [`DataSystems::Validate`] finds.
#[derive(Resource, Debug, Default)]
pub struct DataErrors(Vec<String>);

impl DataErrors {
    /// Adds a problem.
    pub fn push(&mut self, error: impl Into<String>) {
        self.0.push(error.into());
    }

    /// Whether nothing is wrong.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn drain(&mut self) -> impl Iterator<Item = String> + '_ {
        self.0.drain(..)
    }
}

impl<S: Into<String>> Extend<S> for DataErrors {
    fn extend<I: IntoIterator<Item = S>>(&mut self, errors: I) {
        self.0.extend(errors.into_iter().map(Into::into));
    }
}

/// Where the definitions are and how they're read: the folder under
/// `assets/` holding every file, every kind's folder (which names its files
/// too), and the RON options every file is read with.
#[derive(Resource)]
pub(crate) struct Layout {
    pub root: PathBuf,
    pub folders: Vec<&'static str>,
    pub options: ron::Options,
}

/// The steps of a frame, in order. On first load: wait for the files,
/// index, validate, finish. On a reload: note what changed, index,
/// validate, finish.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum Step {
    /// The files still loading (first load).
    Wait,
    /// Files that changed (once loaded).
    Watch,
    /// Every kind indexed, families made whole.
    Index,
    /// Loaded, or failed; or a reload's problems logged.
    Finish,
}

/// Run condition: indexing is due. On first load once every file is
/// loaded; once loaded, after a file changed.
fn indexing(state: Res<State<DataState>>, pending: Res<Pending>, reload: Res<Reload>) -> bool {
    match state.get() {
        DataState::Loading => pending.loaded,
        DataState::Loaded => reload.pending,
        DataState::Failed => false,
    }
}

/// Run condition for [`DataSystems::Validate`]: every kind indexed, on
/// first load without errors, or again after a reload.
fn validating(
    state: Res<State<DataState>>,
    pending: Res<Pending>,
    reload: Res<Reload>,
    errors: Res<DataErrors>,
) -> bool {
    match state.get() {
        // Files that failed to load would show up again as missing
        // references; report the load errors alone.
        DataState::Loading => pending.loaded && errors.is_empty(),
        DataState::Loaded => reload.pending,
        DataState::Failed => false,
    }
}

/// Adds the pipeline: its state and resources, and its steps in order.
/// Kinds are added after (see [`add_kind`]).
pub(crate) fn build(app: &mut App, root: PathBuf, options: ron::Options) {
    if !app.is_plugin_added::<StatesPlugin>() {
        app.add_plugins(StatesPlugin);
    }
    app.init_state::<DataState>()
        .insert_resource(Layout {
            root,
            folders: Vec::new(),
            options,
        })
        .init_resource::<Files>()
        .init_resource::<Pending>()
        .init_resource::<DataErrors>()
        .init_resource::<Reload>()
        .add_message::<DataReloaded>()
        .configure_sets(
            Update,
            (
                Step::Wait.run_if(in_state(DataState::Loading)),
                Step::Watch.run_if(in_state(DataState::Loaded)),
                Step::Index.run_if(indexing),
                DataSystems::Validate.run_if(validating),
                Step::Finish.run_if(indexing),
            )
                .chain(),
        )
        .add_systems(Startup, load::list)
        .add_systems(
            Update,
            (
                load::wait.in_set(Step::Wait),
                finish::finish_load
                    .in_set(Step::Finish)
                    .run_if(in_state(DataState::Loading)),
                finish::finish_reload
                    .in_set(Step::Finish)
                    .run_if(in_state(DataState::Loaded)),
            ),
        );
}

/// Adds kind `T`: its loader, its registry, loading its files once they're
/// listed, indexing it, and watching its files for changes.
pub(crate) fn add_kind<T: Definition>(app: &mut App, registry: Registry<T>, options: ron::Options) {
    crate::asset::add_loader::<T>(app, options);
    app.insert_resource(registry)
        .add_systems(Startup, load::load_kind::<T>.after(load::list))
        .add_systems(
            Update,
            (
                finish::watch_kind::<T>.in_set(Step::Watch),
                index::index_kind::<T>.in_set(Step::Index),
            ),
        );
}
