use core::borrow::Borrow;
use core::hash::Hash;

use bevy::asset::AssetPath;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::Registry;
use crate::Definition;

/// The definitions of kind `T`, by id: a system param, as
/// [`Assets`](bevy::asset::Assets) is for assets.
///
/// Empty until the definitions are first indexed: readable in
/// [`DataSystems::Validate`](crate::DataSystems::Validate), before
/// they're validated; wait for [`DataState::Loaded`](crate::DataState::Loaded)
/// to read them validated. What it reads is whole: what a definition takes from
/// its family is filled in, and the families themselves aren't read, nor a
/// definition whose bases don't resolve; after a hot reload, one whose bases
/// stopped resolving is read as it last was, the error logged. It's what
/// was last indexed: a file changed on disk is read once it's indexed and
/// validated again. A reload's validation errors are logged, not rolled
/// back: what failed them is read.
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
}

impl<T: Definition> Definitions<'_, T> {
    /// The definition with this id, whole. `None` if there's none, or it's
    /// a family.
    pub fn get<Q>(&self, id: &Q) -> Option<&T>
    where
        T::Id: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.registry.defs.get(id).map(AsRef::as_ref)
    }

    /// Whether there's a definition with this id (not a family): whether
    /// [`get`](Self::get) finds one.
    pub fn contains<Q>(&self, id: &Q) -> bool
    where
        T::Id: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.registry.defs.contains_key(id)
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
            .defs
            .iter()
            .map(|(id, def)| (id, def.as_ref()))
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
        self.registry.defs.len()
    }

    /// Whether there are none, families left out.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registry.defs.is_empty()
    }
}

impl<'a, T: Definition> IntoIterator for &'a Definitions<'_, T> {
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;
    type Item = (&'a T::Id, &'a T);

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}
