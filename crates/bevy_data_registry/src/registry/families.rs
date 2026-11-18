//! Families: a definition that builds on another (its `base`) takes what
//! it doesn't say from it. Resolved once every file is indexed (and again
//! on each reload), so what's read is whole.

use bevy::platform::collections::{HashMap, HashSet};

use crate::Definition;

/// Every definition of `defs` that builds on another, made whole: its
/// bases' (each whole first) merged into it, by id; and what's wrong: a
/// base that isn't there, bases that come round to themselves. What builds
/// on nothing isn't in it: it's whole as written. Of an id used twice,
/// only the first counts, as the registry keeps.
pub(crate) fn resolve<T: Definition>(defs: &[&T]) -> (HashMap<T::Id, T>, Vec<String>) {
    let mut by_id: HashMap<&T::Id, &T> = HashMap::default();
    for &def in defs {
        by_id.entry(def.id()).or_insert(def);
    }
    let mut whole = HashMap::default();
    let mut errors = Vec::new();
    for &def in defs {
        let first = by_id
            .get(def.id())
            .is_some_and(|first| core::ptr::eq(*first, def));
        if first && def.base().is_some() {
            let mut visiting = HashSet::default();
            if let Err(error) = make(def, &by_id, &mut whole, &mut visiting) {
                errors.push(format!("{} `{}`: {error}", T::NAME, def.id()));
            }
        }
    }
    (whole, errors)
}

/// `def`, whole, into `whole` (its bases first); `visiting` the chain so
/// far, to catch a cycle.
fn make<T: Definition>(
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
    let base = whole.get(base_id).unwrap_or(base);
    let filled = def
        .inherit(base)
        .ok_or_else(|| format!("builds on `{base_id}`, but {} has no families", T::NAME))?;
    whole.insert(id.clone(), filled);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::Inherit;

    /// A definition with a value it takes from its base when it has none.
    #[derive(crate::Definition, Debug, Clone, PartialEq, Deserialize)]
    #[definition(file = "things")]
    struct Thing {
        id: String,
        #[definition(base)]
        base: Option<String>,
        value: Option<u32>,
    }

    impl Inherit for Thing {
        fn inherit(&mut self, base: &Self) {
            self.value = self.value.or(base.value);
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
        let (whole, errors) = resolve(&refs);
        assert_eq!(whole["child"].value, Some(7), "through its parent");
        assert_eq!(whole["own"].value, Some(3), "its own wins");
        assert!(!whole.contains_key("grandparent"), "whole as written");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors.iter().any(|error| error.contains("nobody")));
        assert!(errors.iter().any(|error| error.contains("itself")));
    }

    #[test]
    fn an_id_used_twice_builds_on_the_first() {
        let things = [
            thing("base", None, Some(1)),
            thing("base", None, Some(2)),
            thing("child", Some("base"), None),
        ];
        let refs: Vec<&Thing> = things.iter().collect();
        assert_eq!(resolve(&refs).0["child"].value, Some(1));
    }

    #[test]
    fn a_later_duplicate_isnt_made_whole_in_the_firsts_place() {
        let things = [
            thing("base", None, Some(1)),
            thing("x", None, Some(3)),
            thing("x", Some("base"), None),
            thing("x", Some("nobody"), None),
        ];
        let refs: Vec<&Thing> = things.iter().collect();
        let (whole, errors) = resolve(&refs);
        assert!(!whole.contains_key("x"), "the first is whole as written");
        assert_eq!(
            errors,
            Vec::<String>::new(),
            "the duplicates aren't resolved"
        );
    }
}
