//! Families: a definition that builds on another (its `base`) takes what
//! it doesn't say from it. Resolved once every file is indexed (and again
//! on each reload), so what's read is whole.

use bevy::platform::collections::{HashMap, HashSet};

use crate::{Definition, Families};

/// Every definition of `defs` that builds on another, as `families` says,
/// made whole: its bases' (each whole first) merged into it, by id; and
/// what's wrong: a base that isn't there, bases that come round to
/// themselves. What builds on nothing isn't in it: it's whole as written.
/// Of an id used twice, only the first counts, as the registry keeps. The
/// kind is `name`, and ids are shown in messages by `label`.
pub(crate) fn resolve<T: Definition>(
    defs: &[&T],
    families: &Families<T>,
    name: &str,
    label: &dyn Fn(&T::Id) -> String,
) -> (HashMap<T::Id, T>, Vec<String>) {
    let mut resolver = Resolver::new(families, label);
    let errors = resolver.run(defs, name);
    (resolver.whole, errors)
}

/// Making a kind's definitions whole: what's known so far.
struct Resolver<'a, T: Definition> {
    families: &'a Families<T>,
    /// Shows ids in messages.
    label: &'a dyn Fn(&T::Id) -> String,
    /// The first definition of each id.
    by_id: HashMap<&'a T::Id, &'a T>,
    /// Those made whole.
    whole: HashMap<T::Id, T>,
    /// Those that can't be, with why: a chain is walked once however many
    /// build on it.
    failed: HashMap<&'a T::Id, String>,
    /// How many steps up a chain of bases the walks have taken.
    steps: usize,
}

impl<'a, T: Definition> Resolver<'a, T> {
    /// A resolver of definitions building on others as `families` says,
    /// showing ids in messages by `label`.
    fn new(families: &'a Families<T>, label: &'a dyn Fn(&T::Id) -> String) -> Self {
        Self {
            families,
            label,
            by_id: HashMap::default(),
            whole: HashMap::default(),
            failed: HashMap::default(),
            steps: 0,
        }
    }

    /// Makes every definition of `defs` that builds on another whole, into
    /// `whole`; returns what's wrong, the kind named `name`.
    fn run(&mut self, defs: &[&'a T], name: &str) -> Vec<String> {
        for &def in defs {
            self.by_id.entry(def.id()).or_insert(def);
        }
        let mut errors = Vec::new();
        for &def in defs {
            let first = self
                .by_id
                .get(def.id())
                .is_some_and(|first| core::ptr::eq(*first, def));
            if first
                && (self.families.base)(def).is_some()
                && let Err(error) = self.make(def)
            {
                errors.push(format!("{name} `{}`: {error}", (self.label)(def.id())));
            }
        }
        errors
    }

    /// `def`, whole, into `whole`: its chain of bases walked up to one
    /// whole or building on nothing, then each made whole from the top
    /// down. Without recursion, so however long a chain is, it can't
    /// overflow the stack.
    fn make(&mut self, def: &'a T) -> Result<(), String> {
        // Those to make whole, `def` first, each building on the next.
        let mut chain: Vec<&'a T> = Vec::new();
        let result = self.walk(def, &mut chain);
        if let Err(error) = &result {
            for def in chain {
                self.failed.insert(def.id(), error.clone());
            }
            return result;
        }
        // Top down, so each base is whole before what builds on it: every
        // base is there, as the walk found.
        while let Some(def) = chain.pop() {
            let Some(base_id) = (self.families.base)(def) else {
                continue;
            };
            let base = self
                .whole
                .get(base_id)
                .or_else(|| self.by_id.get(base_id).copied());
            if let Some(base) = base {
                let filled = (self.families.inherit)(def, base);
                self.whole.insert(def.id().clone(), filled);
            }
        }
        Ok(())
    }

    /// Walks up from `def` into `chain` while what it reaches builds on
    /// something and isn't whole yet.
    fn walk(&mut self, def: &'a T, chain: &mut Vec<&'a T>) -> Result<(), String> {
        let mut seen: HashSet<&T::Id> = HashSet::default();
        let mut current = def;
        while let Some(base_id) = (self.families.base)(current) {
            self.steps += 1;
            if self.whole.contains_key(current.id()) {
                break;
            }
            if let Some(error) = self.failed.get(current.id()) {
                return Err(error.clone());
            }
            if !seen.insert(current.id()) {
                return Err("builds on itself, through its bases".to_owned());
            }
            chain.push(current);
            current = self.by_id.get(base_id).ok_or_else(|| {
                format!("builds on `{}`, which isn't there", (self.label)(base_id))
            })?;
        }
        Ok(())
    }
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

    /// Thing's families, as derived.
    fn families() -> Families<Thing> {
        Thing::info().families.expect("it has a base")
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
        let (whole, errors) = resolve(&refs, &families(), "Thing", &String::clone);
        assert_eq!(whole["child"].value, Some(7), "through its parent");
        assert_eq!(whole["own"].value, Some(3), "its own wins");
        assert!(!whole.contains_key("grandparent"), "whole as written");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors.iter().any(|error| error.contains("nobody")));
        assert!(errors.iter().any(|error| error.contains("itself")));
    }

    #[test]
    fn a_long_chain_doesnt_overflow_the_stack() {
        const LONG: usize = 200_000;
        let mut things = vec![thing("0", None, Some(9))];
        things.extend((1..LONG).map(|n| {
            let mut child = thing(&n.to_string(), None, None);
            child.base = Some((n - 1).to_string());
            child
        }));
        // The deepest first: made whole from it, through every base.
        things.reverse();
        let refs: Vec<&Thing> = things.iter().collect();
        let (whole, errors) = resolve(&refs, &families(), "Thing", &String::clone);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(whole[&(LONG - 1).to_string()].value, Some(9));
        assert_eq!(whole.len(), LONG - 1);
    }

    #[test]
    fn a_long_chain_to_nothing_is_walked_once() {
        const LONG: usize = 20_000;
        let mut things: Vec<Thing> = (0..LONG)
            .map(|n| {
                let base = match n {
                    0 => "nobody".to_owned(),
                    n => (n - 1).to_string(),
                };
                thing(&n.to_string(), Some(&base), None)
            })
            .collect();
        things.reverse();
        let refs: Vec<&Thing> = things.iter().collect();
        let families = families();
        let mut resolver = Resolver::new(&families, &String::clone);
        let errors = resolver.run(&refs, "Thing");
        assert!(resolver.whole.is_empty());
        assert_eq!(errors.len(), LONG, "each says why");
        assert!(errors.iter().all(|error| error.contains("`nobody`")));
        // Walked again from each, it'd take 200 million steps.
        assert!(resolver.steps <= 2 * LONG, "{} steps", resolver.steps);
    }

    #[test]
    fn an_id_used_twice_builds_on_the_first() {
        let things = [
            thing("base", None, Some(1)),
            thing("base", None, Some(2)),
            thing("child", Some("base"), None),
        ];
        let refs: Vec<&Thing> = things.iter().collect();
        assert_eq!(
            resolve(&refs, &families(), "Thing", &String::clone).0["child"].value,
            Some(1)
        );
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
        let (whole, errors) = resolve(&refs, &families(), "Thing", &String::clone);
        assert!(!whole.contains_key("x"), "the first is whole as written");
        assert_eq!(
            errors,
            Vec::<String>::new(),
            "the duplicates aren't resolved"
        );
    }
}
