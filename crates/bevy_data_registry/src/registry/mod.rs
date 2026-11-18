//! Reading loaded data: [`Registry`] holds where each definition of a kind
//! is by id (and those that build on a family, made whole);
//! [`Definitions`] reads them.

mod definitions;
mod families;

use bevy::asset::AssetPath;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

pub use self::definitions::Definitions;
use crate::{DataFile, Definition};

/// Where every definition of kind `T` is, by id; read it through
/// [`Definitions`].
#[derive(Resource)]
pub struct Registry<T: Definition> {
    locations: HashMap<T::Id, Location>,
    /// Those that build on a family, made whole.
    whole: HashMap<T::Id, T>,
    /// The families themselves, never read.
    families: HashSet<T::Id>,
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
            whole: HashMap::default(),
            families: HashSet::default(),
        }
    }
}

impl<T: Definition> Registry<T> {
    /// Indexes, from scratch, every definition of kind `T` in `files`,
    /// in order, and makes those that build on a family whole. Returns ids
    /// used twice (the first kept) and families that don't resolve.
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
        let mut kept: Vec<&T> = Vec::with_capacity(located.len());
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
            kept.push(*def);
            self.locations.insert(
                def.id().clone(),
                Location {
                    file: location.file,
                    path: location.path.clone(),
                    at: location.at,
                },
            );
        }

        self.families = kept
            .iter()
            .filter(|def| def.is_family())
            .map(|def| def.id().clone())
            .collect();
        let (whole, family_errors) = families::resolve(&kept);
        self.whole = whole;
        errors.extend(family_errors);
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
        let mut assets = Assets::<DataFile>::default();
        let files: Vec<(AssetId<DataFile>, AssetPath<'static>)> = files
            .into_iter()
            .enumerate()
            .map(|(n, defs)| {
                let id = assets.add(DataFile::of_defs(defs)).id();
                (id, AssetPath::from(format!("{n}.ron")))
            })
            .collect();
        let mut registry = Registry::default();
        let errors = registry.index(&files, &assets);
        (registry, errors)
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
            !registry.families.contains("stone"),
            "the first isn't a family"
        );
        assert!(registry.families.contains("rocks"));
        assert!(
            !registry.whole.contains_key("x"),
            "the first is whole as written"
        );
        assert_eq!(registry.locations["x"].path, AssetPath::from("0.ron"));
    }
}
