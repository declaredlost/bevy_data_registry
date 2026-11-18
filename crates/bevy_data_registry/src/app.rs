//! Adding kinds to an app: [`DataApp`].

use bevy::prelude::*;

use crate::file::Kinds;
use crate::{Definition, pipeline};

/// Adds kinds of definitions to an [`App`](bevy::app::App), as
/// [`AssetApp`](bevy::asset::AssetApp) adds assets.
pub trait DataApp {
    /// Adds kind `T`: its definitions are read from every data file it
    /// claims ([`T::resolvers`](Definition::resolvers)) and indexed by id.
    /// Adding it again does nothing.
    ///
    /// # Panics
    ///
    /// If `T` claims no files.
    fn register_definition<T: Definition>(&mut self) -> &mut Self;
}

impl DataApp for App {
    fn register_definition<T: Definition>(&mut self) -> &mut Self {
        let kinds = self.world_mut().get_resource_or_init::<Kinds>().clone();
        if kinds.add::<T>() {
            pipeline::add_kind::<T>(self);
        }
        self
    }
}
