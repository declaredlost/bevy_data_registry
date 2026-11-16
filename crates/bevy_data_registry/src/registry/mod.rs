//! Reading loaded data: [`Registry`] holds where each definition is by id
//! (and those that build on a family, made whole), [`Defs`] resolves that
//! to definitions.

mod defs;
mod families;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub use self::defs::Defs;
use self::families::Families;
pub use self::families::{Merged, resolve};
use crate::{DataAsset, Definition, FamilyDefinition};

/// Ids used more than once among `defs`: for each later one, its position
/// and the first's, `(first, again)`, in order. The registry refuses
/// them when it indexes; a test over the data files can check the same.
pub fn duplicates<T: Definition>(defs: &[&T]) -> Vec<(usize, usize)> {
    let mut first: HashMap<&T::Id, usize> = HashMap::default();
    defs.iter()
        .enumerate()
        .filter_map(|(at, def)| {
            if let Some(&earlier) = first.get(def.id()) {
                Some((earlier, at))
            } else {
                first.insert(def.id(), at);
                None
            }
        })
        .collect()
}

/// Where every loaded definition of kind `T` is, keyed by id.
///
/// Prefer [`Defs`] for reading definitions; it resolves locations to values.
#[derive(Resource)]
pub struct Registry<T: Definition> {
    locations: HashMap<T::Id, Location<T>>,
    /// Its families, if the kind has them.
    families: Option<Families<T>>,
    /// Every file of this kind, kept to index them again when one
    /// changes.
    files: Vec<Handle<DataAsset<T>>>,
}

/// Where a definition is: its file, and its position in the file's list.
#[derive(Debug)]
pub struct Location<T: Definition> {
    /// The file it was loaded from.
    pub handle: Handle<DataAsset<T>>,
    /// Its position among the file's definitions: 0 for a file holding one.
    pub index: usize,
}

impl<T: Definition> Location<T> {
    /// Its file's path, and its position if the file holds a list:
    /// `data/tags/damage.ron #2`.
    #[must_use]
    pub fn describe(&self, asset_server: &AssetServer, assets: &Assets<DataAsset<T>>) -> String {
        let path = asset_server
            .get_path(&self.handle)
            .map_or_else(|| "<unknown>".to_owned(), |path| path.to_string());
        match assets.get(&self.handle) {
            Some(DataAsset(defs)) if defs.len() > 1 => format!("{path} #{}", self.index + 1),
            _ => path,
        }
    }
}

impl<T: Definition> Default for Registry<T> {
    fn default() -> Self {
        Self {
            locations: HashMap::default(),
            families: None,
            files: Vec::new(),
        }
    }
}

impl<T: FamilyDefinition> Registry<T> {
    /// Empty, for a kind with families.
    pub(crate) fn with_families() -> Self {
        Self {
            families: Some(Families::new()),
            ..default()
        }
    }
}

impl<T: Definition> Registry<T> {
    pub(crate) fn queue(&mut self, handle: Handle<DataAsset<T>>) {
        self.files.push(handle);
    }

    /// Whether `id` is one of its files.
    pub(crate) fn has_file(&self, id: AssetId<DataAsset<T>>) -> bool {
        self.files.iter().any(|file| file.id() == id)
    }

    /// Indexes every definition of every file by its `id` field, from
    /// scratch, and makes those that build on a family whole. Returns ids
    /// used twice, within a file or across files (see [`duplicates`]), and
    /// families that don't resolve.
    pub(crate) fn index(
        &mut self,
        asset_server: &AssetServer,
        assets: &Assets<DataAsset<T>>,
    ) -> Vec<String> {
        // Failed loads are reported where they're found.
        let located: Vec<(Location<T>, &T)> = self
            .files
            .iter()
            .filter_map(|handle| Some((handle, assets.get(handle)?)))
            .flat_map(|(handle, DataAsset(defs))| {
                defs.iter().enumerate().map(|(index, def)| {
                    let location = Location {
                        handle: handle.clone(),
                        index,
                    };
                    (location, def)
                })
            })
            .collect();
        let defs: Vec<&T> = located.iter().map(|(_, def)| *def).collect();
        let mut errors: Vec<String> = duplicates(&defs)
            .into_iter()
            .map(|(first, again)| {
                format!(
                    "duplicate {} id `{}` in {} and {}",
                    T::NAME,
                    defs[again].id(),
                    located[first].0.describe(asset_server, assets),
                    located[again].0.describe(asset_server, assets)
                )
            })
            .collect();
        self.locations.clear();
        for (location, def) in located.iter().rev() {
            // The first of an id wins: inserted last.
            self.locations.insert(
                def.id().clone(),
                Location {
                    handle: location.handle.clone(),
                    index: location.index,
                },
            );
        }
        if let Some(families) = &mut self.families {
            errors.extend(families.index(&defs));
        }
        errors
    }

    /// The definition `id` made whole, if it builds on a family.
    pub(crate) fn whole(&self, id: &T::Id) -> Option<&T> {
        self.families.as_ref()?.whole(id)
    }

    /// Whether `id` is a family, only built on.
    pub(crate) fn is_family(&self, id: &T::Id) -> bool {
        self.families
            .as_ref()
            .is_some_and(|families| families.contains(id))
    }

    /// Returns where the definition with this id is.
    pub fn location(&self, id: &T::Id) -> Option<&Location<T>> {
        self.locations.get(id)
    }

    /// Iterates over every definition's id and location.
    pub fn iter(&self) -> impl Iterator<Item = (&T::Id, &Location<T>)> {
        self.locations.iter()
    }

    /// Returns `true` if a definition with this id exists.
    pub fn contains(&self, id: &T::Id) -> bool {
        self.locations.contains_key(id)
    }

    /// Iterates over all ids of this kind.
    pub fn ids(&self) -> impl Iterator<Item = &T::Id> {
        self.locations.keys()
    }

    /// Number of definitions of this kind.
    #[must_use]
    pub fn len(&self) -> usize {
        self.locations.len()
    }

    /// Returns `true` if there are no definitions of this kind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Def {
        id: String,
    }

    impl Definition for Def {
        type Id = String;

        const FOLDER: &'static str = "defs";
        const NAME: &'static str = "def";

        fn id(&self) -> &String {
            &self.id
        }
    }

    fn defs(ids: &[&str]) -> Vec<Def> {
        ids.iter()
            .map(|id| Def {
                id: (*id).to_owned(),
            })
            .collect()
    }

    #[test]
    fn duplicates_pair_each_repeat_with_the_first() {
        let defs = defs(&["a", "b", "a", "c", "a", "b"]);
        let refs: Vec<&Def> = defs.iter().collect();
        assert_eq!(duplicates(&refs), [(0, 2), (0, 4), (1, 5)]);
    }

    #[test]
    fn distinct_ids_have_no_duplicates() {
        let defs = defs(&["a", "b", "c"]);
        let refs: Vec<&Def> = defs.iter().collect();
        assert_eq!(duplicates(&refs), []);
    }
}
