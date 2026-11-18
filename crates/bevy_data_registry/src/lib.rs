#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

// The derive macro names this crate by path, from inside it too.
extern crate self as bevy_data_registry;

mod app;
mod definition;
mod file;
pub mod format;
mod pipeline;
mod registry;
pub mod resolve;

use std::sync::Arc;

use bevy::asset::{AssetApp, AssetPath};
use bevy::prelude::*;
pub use bevy_data_registry_macros::Definition;

pub use crate::app::DataApp;
pub use crate::definition::{Definition, Inherit};
use crate::file::{DataFile, DataLoader, Kinds};
use crate::format::{DataFormat, Ron};
pub use crate::pipeline::{DataErrors, DataReloaded, DataState, DataSystems};
pub use crate::registry::Definitions;

/// The most used items: `use bevy_data_registry::prelude::*;`.
pub mod prelude {
    pub use crate::{
        DataApp, DataErrors, DataRegistryPlugin, DataReloaded, DataState, DataSystems, Definition,
        Definitions, Inherit,
    };
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
/// });
/// ```
#[derive(Clone)]
pub struct DataRegistryPlugin {
    /// The folder holding the data files, read at any depth: `data`, under
    /// `assets/`, by default. Loaded with Bevy's `load_folder`, so:
    ///
    /// - a file no loader reads is skipped (with a warning if another loader
    ///   reads it), but a file of the app's formats that no kind claims fails
    ///   the folder;
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
}

impl Default for DataRegistryPlugin {
    fn default() -> Self {
        Self {
            path: AssetPath::from("data"),
            formats: vec![Arc::new(Ron::default())],
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
        pipeline::build(app, self.path.clone());
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
