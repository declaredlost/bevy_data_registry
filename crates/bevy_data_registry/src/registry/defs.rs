use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::{Location, Registry};
use crate::{DataAsset, Definition};

/// Read access to definitions of kind `T` by id.
///
/// Values are only available once [`DataState::Loaded`](crate::DataState)
/// is reached.
#[derive(SystemParam)]
pub struct Defs<'w, T: Definition> {
    registry: Res<'w, Registry<T>>,
    assets: Res<'w, Assets<DataAsset<T>>>,
}

impl<T: Definition> Defs<'_, T> {
    /// Returns the definition with this id, whole (what it takes from its
    /// family filled in). None for a family itself.
    pub fn get(&self, id: &T::Id) -> Option<&T> {
        if self.registry.is_family(id) {
            return None;
        }
        self.registry
            .whole(id)
            .or_else(|| self.resolve(id, self.registry.location(id)?))
    }

    /// Iterates over all loaded definitions of this kind, whole, families
    /// left out.
    pub fn iter(&self) -> impl Iterator<Item = (&T::Id, &T)> {
        self.registry
            .iter()
            .filter(|(id, _)| !self.registry.is_family(id))
            .filter_map(|(id, location)| {
                let def = self
                    .registry
                    .whole(id)
                    .or_else(|| self.resolve(id, location))?;
                Some((id, def))
            })
    }

    /// The definition at `location`, if it's still `id`'s: a file changed
    /// on disk is loaded before it's indexed again, and what's at its
    /// index may be another definition till then.
    fn resolve(&self, id: &T::Id, location: &Location<T>) -> Option<&T> {
        self.assets
            .get(&location.handle)?
            .0
            .get(location.index)
            .filter(|def| def.id() == id)
    }
}
