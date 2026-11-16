#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod app;
mod asset;
mod definition;
mod files;
mod pipeline;
mod registry;

use std::path::PathBuf;

use bevy::prelude::*;

pub use crate::app::DataApp;
pub use crate::asset::DataAsset;
pub use crate::definition::{Definition, FamilyDefinition};
pub use crate::files::kind_of;
pub use crate::pipeline::{DataErrors, DataReloaded, DataState, DataSystems};
pub use crate::registry::{Defs, Location, Merged, Registry, duplicates, resolve};

/// Loads definitions from `assets/<root>/`, of the kinds added with
/// [`DataApp`]; see the crate's docs.
pub struct DataRegistryPlugin {
    root: PathBuf,
    options: ron::Options,
}

impl DataRegistryPlugin {
    /// Definitions under `assets/<root>/`, read with RON's defaults.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            options: ron::Options::default(),
        }
    }

    /// Reads every file with `options`.
    #[must_use]
    pub fn with_options(mut self, options: ron::Options) -> Self {
        self.options = options;
        self
    }
}

impl Plugin for DataRegistryPlugin {
    fn build(&self, app: &mut App) {
        pipeline::build(app, self.root.clone(), self.options.clone());
    }
}
