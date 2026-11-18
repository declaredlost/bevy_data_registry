//! Reading loaded data: [`Registry`] holds each definition of a kind by
//! id, as last indexed (those that build on a family made whole);
//! [`Definitions`] reads them.

mod definitions;
mod families;

use std::sync::Arc;

use bevy::asset::AssetPath;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub use self::definitions::Definitions;
use crate::Definition;
use crate::file::DataFile;

/// Every definition of kind `T`, by id, as last indexed; read it through
/// [`Definitions`].
#[derive(Resource)]
pub struct Registry<T: Definition> {
    /// Where each id is first, families included.
    locations: HashMap<T::Id, Location>,
    /// What's read: the first of each id, whole; not the families, nor
    /// those whose bases don't resolve (on a reload, those kept as they
    /// last were). Kept as indexed: a file loaded again isn't read until
    /// it's indexed and validated.
    defs: HashMap<T::Id, Arc<T>>,
}

/// Where a definition is: its file, and its position in it.
struct Location {
    file: AssetId<DataFile>,
    path: AssetPath<'static>,
    at: usize,
}

impl Location {
    /// Its file's path, and its position if the file holds a list:
    /// `data/tags.ron #2`.
    fn describe(&self, file: &DataFile) -> String {
        if file.len() > 1 {
            format!("{} #{}", self.path, self.at + 1)
        } else {
            self.path.to_string()
        }
    }
}

impl<T: Definition> Default for Registry<T> {
    fn default() -> Self {
        Self {
            locations: HashMap::default(),
            defs: HashMap::default(),
        }
    }
}

impl<T: Definition> Registry<T> {
    /// Indexes, from scratch, every definition of kind `T` in `files`,
    /// in order, and makes those that build on a family whole. Returns ids
    /// used twice (the first kept) and families that don't resolve: those
    /// built on them aren't read.
    pub(crate) fn index(
        &mut self,
        files: &[(AssetId<DataFile>, AssetPath<'static>)],
        assets: &Assets<DataFile>,
    ) -> Vec<String> {
        let located: Vec<(Location, &DataFile, &T)> = files
            .iter()
            .filter_map(|(id, path)| Some((*id, path, assets.get(*id)?)))
            .flat_map(|(id, path, file)| {
                file.of::<T>().map(move |(at, def)| {
                    let location = Location {
                        file: id,
                        path: path.clone(),
                        at,
                    };
                    (location, file, def)
                })
            })
            .collect();

        let mut errors = Vec::new();
        self.locations.clear();
        // The first of each id, as kept; later ones are only reported.
        let mut first: HashMap<&T::Id, usize> = HashMap::default();
        let mut kept: Vec<usize> = Vec::with_capacity(located.len());
        for (index, (location, file, def)) in located.iter().enumerate() {
            if let Some(&earlier) = first.get(def.id()) {
                let (earlier, earlier_file, _) = &located[earlier];
                errors.push(format!(
                    "duplicate {} id `{}` in {} and {}",
                    T::NAME,
                    def.id(),
                    earlier.describe(earlier_file),
                    location.describe(file),
                ));
                continue;
            }
            first.insert(def.id(), index);
            kept.push(index);
            self.locations.insert(
                def.id().clone(),
                Location {
                    file: location.file,
                    path: location.path.clone(),
                    at: location.at,
                },
            );
        }

        let defs: Vec<&T> = kept.iter().map(|&index| located[index].2).collect();
        let (mut whole, family_errors) = families::resolve(&defs);
        errors.extend(family_errors);
        // What was read before: a definition whose bases stop resolving on
        // a reload stays as it last was, rather than vanishing mid-game.
        let previous = core::mem::take(&mut self.defs);
        for &index in &kept {
            let (location, file, def) = &located[index];
            if def.is_family() {
                continue;
            }
            let read = if def.base().is_some() {
                // Not there if its bases don't resolve: as it last was, if
                // it was.
                whole
                    .remove(def.id())
                    .map(Arc::new)
                    .or_else(|| previous.get(def.id()).cloned())
            } else {
                file.shared::<T>(location.at)
            };
            if let Some(read) = read {
                self.defs.insert(def.id().clone(), read);
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::Inherit;

    #[derive(crate::Definition, Debug, Clone, Deserialize)]
    #[definition(file = "things")]
    struct Thing {
        id: String,
        #[definition(base)]
        base: Option<String>,
        #[definition(family)]
        family: bool,
        weight: Option<u32>,
    }

    impl Inherit for Thing {
        fn inherit(&mut self, base: &Self) {
            self.weight = self.weight.or(base.weight);
        }
    }

    fn thing(id: &str, base: Option<&str>, family: bool, weight: Option<u32>) -> Thing {
        Thing {
            id: id.to_owned(),
            base: base.map(str::to_owned),
            family,
            weight,
        }
    }

    /// `files`, indexed in order, each at `<n>.ron`.
    fn index(files: Vec<Vec<Thing>>) -> (Registry<Thing>, Vec<String>) {
        let mut registry = Registry::default();
        let errors = reindex(&mut registry, files);
        (registry, errors)
    }

    /// `files` indexed again into `registry`, as on a reload.
    fn reindex(registry: &mut Registry<Thing>, files: Vec<Vec<Thing>>) -> Vec<String> {
        let mut assets = Assets::<DataFile>::default();
        let files: Vec<(AssetId<DataFile>, AssetPath<'static>)> = files
            .into_iter()
            .enumerate()
            .map(|(n, defs)| {
                let id = assets.add(DataFile::of_defs(defs)).id();
                (id, AssetPath::from(format!("{n}.ron")))
            })
            .collect();
        registry.index(&files, &assets)
    }

    #[test]
    fn a_later_duplicate_neither_hides_nor_replaces_the_first() {
        let (registry, errors) = index(vec![
            vec![
                thing("rocks", None, true, Some(5)),
                thing("stone", None, false, Some(1)),
                thing("x", None, false, Some(3)),
            ],
            vec![
                thing("stone", None, true, None),
                thing("x", Some("rocks"), false, None),
            ],
        ]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors[0].contains("`stone` in 0.ron #2 and 1.ron #1"),
            "{errors:?}"
        );
        assert!(
            registry.defs.contains_key("stone"),
            "the first isn't a family"
        );
        assert!(!registry.defs.contains_key("rocks"), "a family isn't read");
        assert_eq!(
            registry.defs["x"].weight,
            Some(3),
            "the first is whole as written"
        );
        assert_eq!(registry.locations["x"].path, AssetPath::from("0.ron"));
    }

    #[test]
    fn what_builds_on_what_isnt_there_isnt_read() {
        let (registry, errors) = index(vec![vec![
            thing("rocks", None, true, Some(5)),
            thing("pebble", Some("rocks"), false, None),
            thing("orphan", Some("nobody"), false, None),
        ]]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(registry.defs["pebble"].weight, Some(5));
        assert!(!registry.defs.contains_key("orphan"));
        assert!(
            registry.locations.contains_key("orphan"),
            "its path is still known, for messages"
        );
    }

    #[test]
    fn what_stops_resolving_on_a_reload_stays_as_it_was() {
        let (mut registry, _) = index(vec![vec![
            thing("rocks", None, true, Some(5)),
            thing("pebble", Some("rocks"), false, None),
        ]]);
        let errors = reindex(
            &mut registry,
            vec![vec![
                thing("rocks", None, true, Some(5)),
                thing("pebble", Some("rock"), false, Some(1)),
            ]],
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(registry.defs["pebble"].weight, Some(5), "as it last was");
    }
}
