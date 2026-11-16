//! Adding kinds to an app: [`DataApp`].

use bevy::prelude::*;

use crate::definition::{Definition, FamilyDefinition};
use crate::pipeline::{self, Layout};
use crate::registry::Registry;

/// Adds kinds of definitions to an app, after
/// [`DataRegistryPlugin`](crate::DataRegistryPlugin): each kind's files are
/// listed, loaded and indexed with the rest.
pub trait DataApp {
    /// Adds kind `T`.
    fn add_definitions<T: Definition>(&mut self) -> &mut Self;

    /// Adds kind `T`, whose definitions can build on families.
    fn add_family_definitions<T: FamilyDefinition>(&mut self) -> &mut Self;
}

impl DataApp for App {
    fn add_definitions<T: Definition>(&mut self) -> &mut Self {
        add::<T>(self, Registry::<T>::default())
    }

    fn add_family_definitions<T: FamilyDefinition>(&mut self) -> &mut Self {
        add::<T>(self, Registry::<T>::with_families())
    }
}

fn add<T: Definition>(app: &mut App, registry: Registry<T>) -> &mut App {
    let mut layout = app
        .world_mut()
        .get_resource_mut::<Layout>()
        .unwrap_or_else(|| panic!("add DataRegistryPlugin before the {} definitions", T::NAME));
    assert!(
        !layout.folders.contains(&T::FOLDER),
        "{} definitions added twice, or their folder `{}` taken",
        T::NAME,
        T::FOLDER
    );
    layout.folders.push(T::FOLDER);
    let options = layout.options.clone();
    pipeline::add_kind::<T>(app, registry, options);
    app
}
