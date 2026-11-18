//! The loading pipeline, step by step each frame: load the data folder,
//! wait for every file in it, index each kind by id (families made whole),
//! validate across kinds, then finish: loaded, or failed and exiting.
//!
//! Once loaded, a file that changes, is added or removed (hot reload, with
//! `dev`) is loaded again; every kind is then indexed and validated again,
//! problems logged, and [`DataReloaded`] sent.

mod finish;
mod index;
mod load;

use bevy::asset::AssetPath;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

pub use self::finish::DataReloaded;
pub(crate) use self::load::DataFolder;
use self::load::{DataFiles, Progress};
use crate::Definition;
use crate::registry::Registry;

/// Where loading the definitions is: still loading, loaded, or failed.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataState {
    /// The data files are loading.
    #[default]
    Loading,
    /// Every file is loaded, indexed and validated.
    Loaded,
    /// Something was wrong: logged, and the app told to exit.
    Failed,
}

/// Where your systems hook into loading, such as validating across kinds.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataSystems {
    /// Checks across kinds, once every kind is indexed: when they've first
    /// loaded without errors, and after every hot reload. Push what's
    /// wrong to [`DataErrors`]: on first load, any stops the app; after a
    /// reload, they're logged.
    Validate,
}

/// Every problem found while loading and validating the definitions.
///
/// Files that failed to load, ids used twice, families that don't
/// resolve, and what [`DataSystems::Validate`] finds.
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

    /// The problems found so far.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
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

/// The steps of a frame, in order: settle (every file loaded), index,
/// validate, finish.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum Step {
    /// Every file loaded, or failed to.
    Settle,
    /// Every kind indexed, families made whole.
    Index,
    /// Loaded, or failed; or a reload's problems logged.
    Finish,
}

/// Run condition: every file is settled, and indexing is due.
fn due(progress: Res<Progress>) -> bool {
    progress.due
}

/// Run condition for [`DataSystems::Validate`]: every kind indexed, on
/// first load without errors, or again after a reload.
fn validating(
    state: Res<State<DataState>>,
    progress: Res<Progress>,
    errors: Res<DataErrors>,
) -> bool {
    // Files that failed to load would show up again as missing references:
    // on first load, report the load errors alone.
    progress.due && (*state.get() == DataState::Loaded || errors.is_empty())
}

/// Adds the pipeline, loading the folder at `path`: its state and
/// resources, and its steps in order. Kinds are added on their own (see
/// [`add_kind`]).
pub(crate) fn build(app: &mut App, path: AssetPath<'static>) {
    if !app.is_plugin_added::<StatesPlugin>() {
        app.add_plugins(StatesPlugin);
    }
    app.init_state::<DataState>()
        .insert_resource(DataFolder::new(path))
        .init_resource::<DataFiles>()
        .init_resource::<Progress>()
        .init_resource::<DataErrors>()
        .add_message::<DataReloaded>()
        .configure_sets(
            Update,
            (
                Step::Settle.run_if(not(in_state(DataState::Failed))),
                Step::Index.run_if(due),
                DataSystems::Validate.run_if(validating),
                Step::Finish.run_if(due),
            )
                .chain(),
        )
        .add_systems(Startup, load::load)
        .add_systems(
            Update,
            (
                load::settle.in_set(Step::Settle),
                finish::finish_load
                    .in_set(Step::Finish)
                    .run_if(in_state(DataState::Loading)),
                finish::finish_reload
                    .in_set(Step::Finish)
                    .run_if(in_state(DataState::Loaded)),
            ),
        );
}

/// Adds kind `T`'s registry, and indexing it.
pub(crate) fn add_kind<T: Definition>(app: &mut App) {
    app.init_resource::<Registry<T>>()
        .add_systems(Update, index::index_kind::<T>.in_set(Step::Index));
}
