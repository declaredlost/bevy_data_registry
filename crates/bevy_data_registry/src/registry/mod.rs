//! Reading loaded data: [`Registry`] holds each definition of a kind by
//! id, as last indexed (those that build on a family made whole);
//! [`Definitions`] reads them.

mod definitions;
mod families;

use std::sync::Arc;

use bevy::asset::AssetPath;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

pub use self::definitions::Definitions;
use crate::file::DataFile;
use crate::references::Reference;
use crate::{Definition, Families, Referenced};

/// Every definition of kind `T`, by id, as last indexed; read it through
/// [`Definitions`].
#[derive(Resource)]
pub struct Registry<T: Definition> {
    /// The kind's name, in messages.
    name: &'static str,
    /// How its definitions build on others, if they do.
    families: Option<Families<T>>,
    /// How to find the references in one of its definitions.
    references: fn(&T, &mut Referenced),
    /// The ids of its families, as last indexed.
    family_ids: HashSet<T::Id>,
    /// Those kept from before a reload, as their bases stopped resolving,
    /// with the path of the file they were read from then.
    kept: HashMap<T::Id, AssetPath<'static>>,
    /// Where each id is first, families included.
    locations: HashMap<T::Id, Location>,
    /// What's read: the first of each id, whole; not the families, nor
    /// those whose bases don't resolve (on a reload, those kept as they
    /// last were). Kept as indexed: a file loaded again isn't read until
    /// it's indexed and validated.
    defs: HashMap<T::Id, Arc<T>>,
    /// The names of the kind's [`Id`](crate::Id)s, by hash, as read in
    /// any file: its own and references to it.
    names: HashMap<u64, Box<str>>,
}

/// What an id names, to a reference: whether it's somewhere
/// [`Definitions::get`] finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// A definition [`Definitions::get`] finds.
    Read,
    /// A family: only built on, never read.
    Family,
    /// One whose bases don't resolve: an error already.
    Unresolved,
    /// Nothing.
    Missing,
}

/// The references in one definition: its id by name, where it is, and
/// every [`Id`](crate::Id) in it but its own and its base.
pub(crate) struct Referencing {
    pub id: String,
    /// The path of the file it was read from: as kept, if it was.
    pub path: AssetPath<'static>,
    /// Whether it's kept from before a reload, its bases since broken.
    pub kept: bool,
    /// Its file, for the names its references were read from.
    pub file: AssetId<DataFile>,
    pub references: Vec<Reference>,
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

impl<T: Definition> Registry<T> {
    /// An empty registry of the kind named `name`, its definitions building
    /// on others as `families` says.
    pub(crate) fn new(
        name: &'static str,
        families: Option<Families<T>>,
        references: fn(&T, &mut Referenced),
    ) -> Self {
        Self {
            name,
            families,
            references,
            family_ids: HashSet::default(),
            kept: HashMap::default(),
            locations: HashMap::default(),
            defs: HashMap::default(),
            names: HashMap::default(),
        }
    }
}

impl<T: Definition> Registry<T> {
    /// What `id` names, to a reference to it.
    pub(crate) fn target(&self, id: &T::Id) -> Target {
        if self.defs.contains_key(id) {
            return Target::Read;
        }
        if self.family_ids.contains(id) {
            Target::Family
        } else if self.locations.contains_key(id) {
            // Kept, but its bases don't resolve.
            Target::Unresolved
        } else {
            Target::Missing
        }
    }

    /// The references in every definition: what [`Definitions::get`] gives
    /// (whole, or kept from before a reload), and the rest as written in
    /// `assets`.
    pub(crate) fn referenced(&self, assets: &Assets<DataFile>) -> Vec<Referencing> {
        self.locations
            .iter()
            .filter_map(|(id, location)| {
                let def = match self.defs.get(id) {
                    Some(def) => &**def,
                    None => assets.get(location.file)?.get::<T>(location.at)?,
                };
                let mut to = Referenced::default();
                (self.references)(def, &mut to);
                let kept = self.kept.get(id);
                Some(Referencing {
                    id: self.label(id),
                    path: kept.unwrap_or(&location.path).clone(),
                    kept: kept.is_some(),
                    file: location.file,
                    references: to.into_found(),
                })
            })
            .collect()
    }

    /// The name `id` was read from, if it's an [`Id`](crate::Id) and was.
    pub(crate) fn name(&self, id: &T::Id) -> Option<&str> {
        let id: &dyn core::any::Any = id;
        let id = id.downcast_ref::<crate::Id<T>>()?;
        self.names.get(&id.hash()).map(AsRef::as_ref)
    }

    /// `id` for messages: by its name if it has one, as it shows if not.
    fn label(&self, id: &T::Id) -> String {
        self.name(id)
            .map_or_else(|| id.to_string(), crate::id::escaped)
    }

    /// Keeps the names of the kind's [`Id`]s read in `files`; returns two
    /// names with one hash, as errors.
    fn name_ids(&mut self, files: &[&DataFile]) -> Vec<String> {
        let mut errors = Vec::new();
        self.names.clear();
        for file in files {
            for (hash, name) in file.names().of(core::any::TypeId::of::<T>()) {
                match self.names.get(&hash) {
                    Some(kept) if **kept != *name => errors.push(format!(
                        "{} ids `{kept}` and `{name}` have the same hash ({hash:#018x}): rename one",
                        self.name
                    )),
                    Some(_) => {}
                    None => {
                        self.names.insert(hash, name.into());
                    }
                }
            }
        }
        errors
    }

    /// Indexes, from scratch, every definition of kind `T` in `files`,
    /// in order, and makes those that build on a family whole. Returns ids
    /// used twice (the first kept), two names with one hash, and families
    /// that don't resolve: those built on them aren't read.
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

        let all: Vec<&DataFile> = files.iter().filter_map(|(id, _)| assets.get(*id)).collect();
        let mut errors = self.name_ids(&all);
        // Where each was, for one kept as it was.
        let mut was: HashMap<T::Id, AssetPath<'static>> = self
            .locations
            .drain()
            .map(|(id, location)| (id, location.path))
            .collect();
        was.extend(self.kept.drain());
        // The first of each id, as kept; later ones are only reported.
        let mut first: HashMap<&T::Id, usize> = HashMap::default();
        let mut kept: Vec<usize> = Vec::with_capacity(located.len());
        for (index, (location, file, def)) in located.iter().enumerate() {
            if let Some(&earlier) = first.get(def.id()) {
                let (earlier, earlier_file, _) = &located[earlier];
                errors.push(format!(
                    "duplicate {} id `{}` in {} and {}",
                    self.name,
                    self.label(def.id()),
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
        let label = |id: &T::Id| self.label(id);
        let mut whole = HashMap::default();
        if let Some(families) = &self.families {
            let (made, family_errors) = families::resolve(&defs, families, self.name, &label);
            whole = made;
            errors.extend(family_errors);
        }
        let is_family = |def: &T| self.families.is_some_and(|f| (f.is_family)(def));
        let base = |def: &T| self.families.and_then(|f| (f.base)(def)).is_some();
        // What was read before: a definition whose bases stop resolving on
        // a reload stays as it last was, rather than vanishing mid-game.
        let previous = core::mem::take(&mut self.defs);
        self.family_ids.clear();
        for &index in &kept {
            let (location, file, def) = &located[index];
            if is_family(def) {
                self.family_ids.insert(def.id().clone());
                continue;
            }
            let read = if base(def) {
                // Not there if its bases don't resolve: as it last was, if
                // it was.
                whole.remove(def.id()).map(Arc::new).or_else(|| {
                    let kept = previous.get(def.id()).cloned()?;
                    if let Some(path) = was.remove(def.id()) {
                        self.kept.insert(def.id().clone(), path);
                    }
                    Some(kept)
                })
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
    use crate::file::tests::{of_defs, of_defs_named};

    #[derive(crate::Definition, Debug, Clone, Deserialize)]
    #[definition(file = "things")]
    struct Thing {
        id: String,
        #[definition(base)]
        base: Option<String>,
        #[definition(family)]
        family: bool,
        weight: Option<u32>,
        /// A reference, taken from the base if it has none.
        #[serde(default)]
        near: Option<crate::Id<Thing>>,
    }

    impl Inherit for Thing {
        fn inherit(&mut self, base: &Self) {
            self.weight = self.weight.or(base.weight);
            self.near = self.near.or(base.near);
        }
    }

    fn thing(id: &str, base: Option<&str>, family: bool, weight: Option<u32>) -> Thing {
        Thing {
            id: id.to_owned(),
            base: base.map(str::to_owned),
            family,
            weight,
            near: None,
        }
    }

    /// An empty registry of kind `T`, as registering it makes.
    fn registry<T: Definition>() -> Registry<T> {
        let info = T::info();
        Registry::new(info.name, info.families, info.references)
    }

    /// `files`, indexed in order, each at `<n>.ron`.
    fn index(files: Vec<Vec<Thing>>) -> (Registry<Thing>, Vec<String>) {
        let mut registry = registry::<Thing>();
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
                let id = assets.add(of_defs(defs)).id();
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
    fn two_names_of_a_kind_with_one_hash_are_an_error() {
        use core::any::TypeId;

        use crate::id::Names;

        #[derive(crate::Definition, Debug, Deserialize)]
        #[definition(file = "items")]
        struct Item {
            id: crate::Id<Item>,
        }

        let named = |name: &str| {
            let mut names = Names::default();
            names.keep(TypeId::of::<Item>(), 7, name);
            names.keep(TypeId::of::<Thing>(), 7, "a thing's");
            names
        };
        let mut assets = Assets::<DataFile>::default();
        let files: Vec<(AssetId<DataFile>, AssetPath<'static>)> = ["sword", "axe"]
            .into_iter()
            .enumerate()
            .map(|(n, name)| {
                let item = Item {
                    id: crate::Id::from_name(name),
                };
                let file = of_defs_named(vec![item], named(name));
                (assets.add(file).id(), AssetPath::from(format!("{n}.ron")))
            })
            .collect();
        let mut registry = registry::<Item>();
        let errors = registry.index(&files, &assets);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("Item ids `sword` and `axe` have the same hash"),
            "{errors:?}"
        );
    }

    #[test]
    fn references_are_read_from_what_get_gives() {
        let mut rocks = thing("rocks", None, true, Some(5));
        rocks.near = Some(crate::Id::from_name("cliff"));
        let (mut registry, _) = index(vec![vec![
            rocks,
            thing("pebble", Some("rocks"), false, None),
        ]]);
        // `pebble`'s base is gone: it's kept as it was, `near` inherited.
        let mut assets = Assets::<DataFile>::default();
        let file = assets.add(of_defs(vec![thing("pebble", Some("rock"), false, Some(1))]));
        let files = [(file.id(), AssetPath::from("moved.ron"))];
        registry.index(&files, &assets);
        let referenced = registry.referenced(&assets);
        let pebble = referenced
            .iter()
            .find(|from| from.id == "pebble")
            .expect("kept");
        assert_eq!(pebble.references.len(), 1, "its kept `near`");
        assert!(pebble.kept);
        assert_eq!(pebble.path, AssetPath::from("0.ron"), "where it was read");
        assert_eq!(pebble.references[0].field, "near");
        assert_eq!(registry.target(&"pebble".to_owned()), Target::Read);
        assert_eq!(registry.target(&"nothing".to_owned()), Target::Missing);
    }

    #[test]
    fn a_family_is_a_target_of_its_own() {
        let (registry, _) = index(vec![vec![
            thing("rocks", None, true, Some(5)),
            thing("orphan", Some("nobody"), false, None),
        ]]);
        assert_eq!(registry.target(&"rocks".to_owned()), Target::Family);
        assert_eq!(registry.target(&"orphan".to_owned()), Target::Unresolved);
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
