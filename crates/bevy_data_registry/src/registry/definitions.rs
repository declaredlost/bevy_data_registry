use core::borrow::Borrow;
use core::hash::Hash;

use bevy::asset::AssetPath;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::{Location, Registry};
use crate::Definition;
use crate::file::DataFile;

/// The definitions of kind `T`, by id: a system param, as
/// [`Assets`](bevy::asset::Assets) is for assets.
///
/// Empty until [`DataState::Loaded`](crate::DataState::Loaded). What it
/// reads is whole: what a definition takes from its family is filled in,
/// and the families themselves aren't read.
///
/// ```rust,ignore
/// fn price(items: Definitions<Item>) {
///     let sword = items.get("sword").unwrap();
///     for (id, item) in &items { .. }
/// }
/// ```
#[derive(SystemParam)]
pub struct Definitions<'w, T: Definition> {
    registry: Res<'w, Registry<T>>,
    files: Res<'w, Assets<DataFile>>,
}

impl<T: Definition> Definitions<'_, T> {
    /// The definition with this id, whole. `None` if there's none, or it's
    /// a family.
    pub fn get<Q>(&self, id: &Q) -> Option<&T>
    where
        T::Id: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.registry.families.contains(id) {
            return None;
        }
        self.registry
            .whole
            .get(id)
            .or_else(|| self.read(self.registry.locations.get(id)?))
    }

    /// Whether there's a definition with this id (not a family): whether
    /// [`get`](Self::get) finds one.
    pub fn contains<Q>(&self, id: &Q) -> bool
    where
        T::Id: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(id).is_some()
    }

    /// The path of the file the definition with this id is in, families
    /// included: for messages about it.
    pub fn path<Q>(&self, id: &Q) -> Option<&AssetPath<'static>>
    where
        T::Id: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.registry
            .locations
            .get(id)
            .map(|location| &location.path)
    }

    /// Every definition, whole, by id, families left out; in no order.
    pub fn iter(&self) -> impl Iterator<Item = (&T::Id, &T)> {
        self.registry
            .locations
            .iter()
            .filter(|(id, _)| !self.registry.families.contains(*id))
            .filter_map(|(id, location)| {
                let def = self
                    .registry
                    .whole
                    .get(id)
                    .or_else(|| self.read(location))?;
                Some((id, def))
            })
    }

    /// Every id, families left out; in no order. Those
    /// [`iter`](Self::iter) gives.
    pub fn ids(&self) -> impl Iterator<Item = &T::Id> {
        self.iter().map(|(id, _)| id)
    }

    /// How many definitions there are, families left out: those
    /// [`iter`](Self::iter) gives, counted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Whether there are none, families left out.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// The definition at `location`, as written. A file changed on disk is
    /// loaded before it's indexed again, so what's there may be another
    /// definition until it is: the indexed one, or none. Every read goes
    /// through here, so `get`, `contains`, `iter` and `len` agree meanwhile.
    fn read(&self, location: &Location) -> Option<&T> {
        let def = self.files.get(location.file)?.get::<T>(location.at)?;
        self.registry
            .locations
            .get(def.id())
            .is_some_and(|indexed| indexed.file == location.file && indexed.at == location.at)
            .then_some(def)
    }
}

impl<'a, T: Definition> IntoIterator for &'a Definitions<'_, T> {
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;
    type Item = (&'a T::Id, &'a T);

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}
