#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

// The derive macro names this crate by path, from inside it too.
extern crate self as bevy_data_registry;

mod app;
mod definition;
mod file;
mod format;
mod pipeline;
mod registry;
mod resolve;

use std::sync::Arc;

use bevy::asset::{AssetApp, AssetPath};
use bevy::prelude::*;
/// Derives [`Definition`](trait@Definition): its name, the files it claims,
/// its id, and, for a kind with families, its base and family flag. See
/// the crate's docs.
pub use bevy_data_registry_macros::Definition;

pub use crate::app::DataApp;
pub use crate::definition::{Definition, Inherit};
pub use crate::file::{DataFile, FileContents, FileSeed};
use crate::file::{DataLoader, Kinds};
pub use crate::format::{DataFormat, Ron};
pub use crate::pipeline::{DataErrors, DataReloaded, DataState, DataSystems};
pub use crate::registry::Definitions;
pub use crate::resolve::{Extension, FileName, Folder, Resolve};

/// The most used items: `use bevy_data_registry::prelude::*;`.
pub mod prelude {
    pub use crate::{
        DataApp, DataErrors, DataRegistryPlugin, DataReloaded, DataState, DataSystems, Definition,
        Definitions, Extension, FileName, Folder, Inherit, Resolve,
    };
}

/// Loads definitions from every file under [`path`](Self::path), in any of
/// its [`formats`](Self::formats), as the kind added with
/// [`DataApp::register_definition`] that claims it; see the crate's docs.
///
/// ```rust,ignore
/// app.add_plugins(DataRegistryPlugin::default());
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
    /// - every file in it must be one some loader reads: Bevy fails the
    ///   whole folder over one that none does;
    /// - on first load, a file that fails to load fails the folder before
    ///   the files after it are tried, so broken files are reported one at
    ///   a time;
    /// - with `dev`, a folder in it deleted or moved out while the app runs
    ///   isn't noticed, its definitions kept until a restart (deleting its
    ///   files first is);
    /// - a folder in it linking back to one it's in makes loading never
    ///   end.
    pub path: AssetPath<'static>,
    /// The formats data files are written in, each chosen by a file's
    /// extension: [`Ron`] by default.
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
