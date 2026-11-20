#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

// The derive macro names this crate by path, from inside it too.
extern crate self as bevy_data_registry;

mod app;
mod definition;
mod file;
pub mod format;
mod id;
mod pipeline;
mod references;
mod registry;
pub mod resolve;

use std::sync::Arc;

use bevy::asset::{AssetApp, AssetPath};
use bevy::prelude::*;
pub use bevy_data_registry_macros::{Definition, References};

pub use crate::app::DataApp;
pub use crate::definition::{Definition, DefinitionInfo, Families, Inherit};
use crate::file::{DataFile, DataLoader, Kinds};
use crate::format::{DataFormat, Ron};
pub use crate::id::Id;
pub use crate::pipeline::{DataErrors, DataReloaded, DataState, DataSystems};
pub use crate::references::{Referenced, References};
pub use crate::registry::Definitions;

/// The most used items: `use bevy_data_registry::prelude::*;`.
pub mod prelude {
    pub use crate::{
        DataApp, DataErrors, DataRegistryPlugin, DataReloaded, DataState, DataSystems, Definition,
        Definitions, Id, Inherit, References,
    };
}

/// What the derives use. Not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use crate::references::walk::{ViaNothing, ViaReferences, Walk};
}

/// Loads every data file under [`path`](Self::path) as the kind that
/// claims it.
///
/// Files are read in any of its [`formats`](Self::formats), as the kind
/// added with [`DataApp::register_definition`] that claims them; see the
/// crate's docs.
///
/// ```rust,ignore
/// // The defaults: RON files under `assets/data/`.
/// app.add_plugins(DataRegistryPlugin::default());
/// // Or set the folder and the formats.
/// app.add_plugins(DataRegistryPlugin {
///     path: "mods://data".into(),
///     formats: vec![Arc::new(Ron::default()), Arc::new(Json)],
///     ..default()
/// });
/// ```
#[derive(Clone)]
pub struct DataRegistryPlugin {
    /// The folder holding the data files, read at any depth: `data`, under
    /// `assets/`, by default. Loaded with Bevy's `load_folder`, so:
    ///
    /// - every file in it must be one some asset loader reads: Bevy fails the
    ///   folder over one none does (`notes.txt`); one another loader reads is
    ///   skipped, with a warning; so is one whose `.meta` names a loader the
    ///   app doesn't have, without one;
    /// - with `dev`, a file added with a malformed `.meta` fails the folder's
    ///   reload until the folder changes again, or the file itself does: Bevy
    ///   may not reload a file when only its `.meta` changes;
    /// - on first load, a file that fails to load fails the folder before the
    ///   files after it are tried, so broken files are reported one at a time;
    /// - with `dev`, a folder in it deleted or moved out while the app runs
    ///   isn't noticed, its definitions kept until a restart (deleting its
    ///   files first is);
    /// - a folder in it linking back to one it's in makes loading never end.
    pub path: AssetPath<'static>,
    /// The formats data files are written in, each chosen by a file's
    /// extension: [`Ron`] by default. At least one, each with an extension:
    /// the plugin panics otherwise.
    pub formats: Vec<Arc<dyn DataFormat>>,
    /// Whether the app exits, with an error, when loading fails: `true` by
    /// default. The exit is sent once every `OnEnter(DataState::Failed)`
    /// system has run, so they can read [`DataErrors`] first; `false`
    /// leaves ending the app to them, to show what's wrong.
    pub exit_on_failure: bool,
}

impl Default for DataRegistryPlugin {
    fn default() -> Self {
        Self {
            path: AssetPath::from("data"),
            formats: vec![Arc::new(Ron::default())],
            exit_on_failure: true,
        }
    }
}

impl Plugin for DataRegistryPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            !self.formats.is_empty(),
            "DataRegistryPlugin needs at least one format"
        );
        assert!(
            self.formats
                .iter()
                .all(|format| !format.extensions().is_empty()),
            "a format in DataRegistryPlugin::formats has no extensions: none of its files would load"
        );
        let kinds = app.world_mut().get_resource_or_init::<Kinds>().clone();
        let extensions = self
            .formats
            .iter()
            .flat_map(|format| format.extensions().iter().copied())
            .collect();
        app.init_asset::<DataFile>()
            .register_asset_loader(DataLoader {
                kinds,
                root: self.path.clone(),
                formats: self.formats.clone(),
                extensions,
            });
        pipeline::build(app, self.path.clone(), self.exit_on_failure);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "has no extensions")]
    fn a_format_needs_an_extension() {
        App::new().add_plugins(DataRegistryPlugin {
            formats: vec![
                Arc::new(Ron::default()),
                Arc::new(Ron {
                    extensions: Vec::new(),
                    ..Ron::default()
                }),
            ],
            ..default()
        });
    }
}
