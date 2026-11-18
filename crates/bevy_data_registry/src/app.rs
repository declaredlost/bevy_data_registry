//! Adding kinds to an app: [`DataApp`].

use bevy::prelude::*;

use crate::file::Kinds;
use crate::{Definition, DefinitionInfo, pipeline};

/// Adds kinds of definitions to an [`App`](bevy::app::App), as
/// [`AssetApp`](bevy::asset::AssetApp) adds assets.
pub trait DataApp {
    /// Adds kind `T`: its definitions are read from every data file it
    /// claims (its [`DefinitionInfo::resolvers`]) and indexed by id.
    /// Adding it again does nothing.
    ///
    /// # Panics
    ///
    /// If it has no resolvers: whether any files match isn't checked.
    fn register_definition<T: Definition>(&mut self) -> &mut Self;
}

impl DataApp for App {
    fn register_definition<T: Definition>(&mut self) -> &mut Self {
        let kinds = self.world_mut().get_resource_or_init::<Kinds>().clone();
        let DefinitionInfo {
            name,
            resolvers,
            references,
            families,
        } = T::info();
        if kinds.add::<T>(name, resolvers) {
            pipeline::add_kind::<T>(self, name, families, references);
        }
        self
    }
}
