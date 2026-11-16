//! Families: a definition that builds on another (its `base`) takes what
//! it doesn't say from it, as its kind merges them. Resolved once every
//! file is indexed (and again on each reload), so what's read is whole.

use bevy::platform::collections::{HashMap, HashSet};

use crate::{Definition, FamilyDefinition};

/// A kind's families, once indexed: those that build on one made whole,
/// and the families themselves, never read.
pub(crate) struct Families<T: Definition> {
    /// Resolves them, `T` known to have families (which [`Definition`]
    /// alone doesn't say).
    resolve: fn(&[&T]) -> Resolved<T>,
    whole: HashMap<T::Id, T>,
    abstracts: HashSet<T::Id>,
}

/// A kind's families, resolved from every definition of it.
struct Resolved<T: Definition> {
    /// Those that build on one, made whole, by id.
    whole: HashMap<T::Id, T>,
    /// The families themselves.
    abstracts: HashSet<T::Id>,
    /// What's wrong.
    errors: Vec<String>,
}

impl<T: FamilyDefinition> Families<T> {
    pub fn new() -> Self {
        Self {
            resolve: |defs| {
                let families = defs
                    .iter()
                    .filter(|def| def.family())
                    .map(|def| def.id().clone())
                    .collect();
                let merged = resolve(defs);
                Resolved {
                    whole: merged.whole,
                    abstracts: families,
                    errors: merged.errors,
                }
            },
            whole: HashMap::default(),
            abstracts: HashSet::default(),
        }
    }
}

impl<T: Definition> Families<T> {
    /// Resolves them again from every definition of the kind; returns
    /// what's wrong.
    pub fn index(&mut self, defs: &[&T]) -> Vec<String> {
        let resolved = (self.resolve)(defs);
        self.whole = resolved.whole;
        self.abstracts = resolved.abstracts;
        resolved.errors
    }

    /// The definition `id` made whole, if it builds on a family.
    pub fn whole(&self, id: &T::Id) -> Option<&T> {
        self.whole.get(id)
    }

    /// Whether `id` is a family.
    pub fn contains(&self, id: &T::Id) -> bool {
        self.abstracts.contains(id)
    }
}

/// What builds on families, merged: see [`resolve`].
#[derive(Debug)]
pub struct Merged<T: Definition> {
    /// Every definition that builds on another, made whole, by id.
    pub whole: HashMap<T::Id, T>,
    /// What's wrong.
    pub errors: Vec<String>,
}

/// Every definition of `defs` that builds on another, made whole: its
/// bases' (each whole first) merged into it, by id. And what's wrong: a
/// base that isn't there, bases that come round to themselves. What
/// builds on nothing isn't in it: it's whole as written.
pub fn resolve<T: FamilyDefinition>(defs: &[&T]) -> Merged<T> {
    let by_id: HashMap<&T::Id, &T> = defs.iter().map(|def| (def.id(), *def)).collect();
    let mut whole: HashMap<T::Id, T> = HashMap::default();
    let mut errors = Vec::new();
    for &def in defs {
        if def.base().is_some() {
            let mut visiting = HashSet::default();
            if let Err(error) = make(def, &by_id, &mut whole, &mut visiting) {
                errors.push(format!("{} `{}`: {error}", T::NAME, def.id()));
            }
        }
    }
    Merged { whole, errors }
}

/// `def`, whole, into `whole` (its bases first); `visiting` the chain so
/// far, to catch a cycle.
fn make<T: FamilyDefinition>(
    def: &T,
    by_id: &HashMap<&T::Id, &T>,
    whole: &mut HashMap<T::Id, T>,
    visiting: &mut HashSet<T::Id>,
) -> Result<(), String> {
    let id = def.id();
    if whole.contains_key(id) {
        return Ok(());
    }
    let Some(base_id) = def.base() else {
        return Ok(());
    };
    if !visiting.insert(id.clone()) {
        return Err("builds on itself, through its bases".to_owned());
    }
    let base = by_id
        .get(base_id)
        .ok_or_else(|| format!("builds on `{base_id}`, which isn't there"))?;
    make(*base, by_id, whole, visiting)?;
    // Its base whole, if it builds on one itself; as written if not.
    let filled = match whole.get(base_id) {
        Some(whole_base) => def.inherit(whole_base),
        None => def.inherit(base),
    };
    whole.insert(id.clone(), filled);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    /// A definition with a value it takes from its base when it has none.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    struct Thing {
        id: String,
        base: Option<String>,
        value: Option<u32>,
    }

    impl Definition for Thing {
        type Id = String;

        const FOLDER: &'static str = "things";
        const NAME: &'static str = "thing";

        fn id(&self) -> &String {
            &self.id
        }
    }

    impl FamilyDefinition for Thing {
        fn family(&self) -> bool {
            false
        }

        fn base(&self) -> Option<&String> {
            self.base.as_ref()
        }

        fn inherit(&self, base: &Self) -> Self {
            Self {
                value: self.value.or(base.value),
                ..self.clone()
            }
        }
    }

    fn thing(id: &str, base: Option<&str>, value: Option<u32>) -> Thing {
        Thing {
            id: id.to_owned(),
            base: base.map(str::to_owned),
            value,
        }
    }

    #[test]
    fn a_chain_of_families_is_made_whole_and_what_cant_be_says_why() {
        let things = [
            thing("grandparent", None, Some(7)),
            thing("parent", Some("grandparent"), None),
            thing("child", Some("parent"), None),
            thing("own", Some("parent"), Some(3)),
            thing("orphan", Some("nobody"), None),
            thing("a", Some("b"), None),
            thing("b", Some("a"), None),
        ];
        let refs: Vec<&Thing> = things.iter().collect();
        let merged = resolve(&refs);
        assert_eq!(merged.whole["child"].value, Some(7), "through its parent");
        assert_eq!(merged.whole["own"].value, Some(3), "its own wins");
        assert!(
            !merged.whole.contains_key("grandparent"),
            "whole as written"
        );
        assert_eq!(merged.errors.len(), 3, "{:?}", merged.errors);
        assert!(merged.errors.iter().any(|error| error.contains("nobody")));
        assert!(merged.errors.iter().any(|error| error.contains("itself")));
    }
}
