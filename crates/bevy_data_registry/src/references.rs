//! [`References`]: finding the [`Id`]s in a definition, to check each
//! names one; [`Referenced`], what's found.

use core::any::TypeId;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use bevy::platform::collections::{HashMap as BevyHashMap, HashSet as BevyHashSet};

use crate::id::{Id, short_type_name};

/// What can hold references to definitions: [`Id`], and what holds one.
///
/// Derive it for a type of your own a definition holds, so the [`Id`]s in
/// it are checked too:
///
/// ```rust,ignore
/// #[derive(References, Deserialize)]
/// struct Drop {
///     item: Id<Item>,
///     chance: f32,
/// }
/// ```
///
/// Plain values (`String`, numbers, `bool`) are, holding none, so a map
/// keyed by a `String` or a tuple of an [`Id`] and an `f32` is walked.
///
/// A field of a type that isn't one is skipped, without a word: a type of
/// your own holding [`Id`]s needs the derive, or they go unchecked. So is
/// a field marked `#[definition(unchecked)]`. In a generic type, a field
/// is walked if it's `References` by what the type says of its parameters:
/// `Vec<Id<T>>` and `next: Option<Box<Node<T>>>` are, whatever `T` is;
/// `payload: T` is only if `T` is bound by `References`, and the rest of
/// the type is walked all the same.
///
/// An error names the field an [`Id`] is in by its path from the
/// definition: `finds.tag`.
pub trait References {
    /// Visits every [`Id`] in it.
    fn visit_references(&self, to: &mut Referenced);
}

/// The [`Id`]s found in a definition, with the fields they're in, as walked.
#[derive(Debug, Default)]
pub struct Referenced {
    /// The fields being walked, outermost first: `finds`, `tag`.
    path: Vec<&'static str>,
    found: Vec<Reference>,
}

/// An [`Id`] found in a definition: the kind it's an id of, its hash, and
/// the field it's in, as a path from the definition: `finds.tag`.
#[derive(Debug, Clone)]
pub(crate) struct Reference {
    pub field: String,
    pub kind: TypeId,
    /// The kind's type name, if it's no registered kind, for messages.
    pub kind_name: &'static str,
    pub hash: u64,
}

impl Referenced {
    /// Says the [`Id`]s visited until [`leave`](Self::leave) are in
    /// `field`, of what's being walked. Called by the derives.
    #[doc(hidden)]
    pub fn enter(&mut self, field: &'static str) {
        self.path.push(field);
    }

    /// Ends the field [`enter`](Self::enter) began. Called by the derives.
    #[doc(hidden)]
    pub fn leave(&mut self) {
        self.path.pop();
    }

    /// Every [`Id`] found.
    pub(crate) fn into_found(self) -> Vec<Reference> {
        self.found
    }
}

impl<T: 'static> References for Id<T> {
    fn visit_references(&self, to: &mut Referenced) {
        to.found.push(Reference {
            field: to.path.join("."),
            kind: TypeId::of::<T>(),
            kind_name: short_type_name::<T>(),
            hash: self.hash(),
        });
    }
}

impl<T: References, E: References> References for Result<T, E> {
    fn visit_references(&self, to: &mut Referenced) {
        match self {
            Ok(item) => item.visit_references(to),
            Err(error) => error.visit_references(to),
        }
    }
}

impl<T: References> References for Option<T> {
    fn visit_references(&self, to: &mut Referenced) {
        if let Some(item) = self {
            item.visit_references(to);
        }
    }
}

/// [`References`] for what holds one value: it visited.
macro_rules! references_of_pointers {
    ($($ty:ident),*) => {$(
        impl<T: References + ?Sized> References for $ty<T> {
            fn visit_references(&self, to: &mut Referenced) {
                (**self).visit_references(to);
            }
        }
    )*};
}

references_of_pointers!(Box, Rc, Arc);

impl<T: References + ToOwned + ?Sized> References for Cow<'_, T> {
    fn visit_references(&self, to: &mut Referenced) {
        (**self).visit_references(to);
    }
}

/// [`References`] for plain values: none in them.
macro_rules! references_of_none {
    ($($ty:ty),*) => {$(
        impl References for $ty {
            fn visit_references(&self, _: &mut Referenced) {}
        }
    )*};
}

references_of_none!(
    (),
    bool,
    char,
    str,
    String,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64
);

/// [`References`] for tuples: each element visited.
macro_rules! references_of_tuples {
    ($(($($name:ident),+)),*) => {$(
        impl<$($name: References),+> References for ($($name,)+) {
            #[expect(non_snake_case, reason = "the elements, named as their types")]
            fn visit_references(&self, to: &mut Referenced) {
                let ($($name,)+) = self;
                $($name.visit_references(to);)+
            }
        }
    )*};
}

references_of_tuples!((A), (A, B), (A, B, C), (A, B, C, D));

/// [`References`] for collections: each item visited.
macro_rules! references_of_items {
    ($($ty:ty => [$($generics:tt)*];)*) => {$(
        impl<$($generics)*> References for $ty {
            fn visit_references(&self, to: &mut Referenced) {
                for item in self {
                    item.visit_references(to);
                }
            }
        }
    )*};
}

references_of_items! {
    [T] => [T: References];
    [T; N] => [T: References, const N: usize];
    Vec<T> => [T: References];
    VecDeque<T> => [T: References];
    BTreeSet<T> => [T: References];
    HashSet<T, S> => [T: References, S];
    BevyHashSet<T, S> => [T: References, S];
}

/// [`References`] for maps: each key and value visited.
macro_rules! references_of_entries {
    ($($ty:ty => [$($generics:tt)*];)*) => {$(
        impl<$($generics)*> References for $ty {
            fn visit_references(&self, to: &mut Referenced) {
                for (key, value) in self {
                    key.visit_references(to);
                    value.visit_references(to);
                }
            }
        }
    )*};
}

references_of_entries! {
    BTreeMap<K, V> => [K: References, V: References];
    HashMap<K, V, S> => [K: References, V: References, S];
    BevyHashMap<K, V, S> => [K: References, V: References, S];
}

/// What the derives use to visit a field's [`Id`]s if its type is
/// [`References`], and skip it if not.
#[doc(hidden)]
pub mod walk {
    use super::{Referenced, References};

    /// A field, to visit.
    pub struct Walk<'a, T: ?Sized>(pub &'a T);

    /// A field whose type is [`References`]: visited.
    pub trait ViaReferences {
        fn walk(&self, to: &mut Referenced);
    }

    impl<T: References + ?Sized> ViaReferences for &Walk<'_, T> {
        fn walk(&self, to: &mut Referenced) {
            self.0.visit_references(to);
        }
    }

    /// A field whose type isn't: skipped.
    pub trait ViaNothing {
        fn walk(&self, _: &mut Referenced) {}
    }

    impl<T: ?Sized> ViaNothing for Walk<'_, T> {}
}

#[cfg(test)]
mod tests {
    use super::walk::{ViaNothing as _, ViaReferences as _, Walk};
    use super::*;

    struct Item;

    /// A type of the app's own, with the derive.
    #[derive(crate::References)]
    struct Find {
        tag: Id<Item>,
    }

    /// A recursive generic type: walked, though its impl can't be bound by
    /// itself.
    #[derive(crate::References)]
    struct Node<T: 'static> {
        item: Id<T>,
        next: Option<Box<Node<T>>>,
        /// `Note` isn't `References`: skipped, as it'd be in any type.
        outcome: Result<Id<T>, Note>,
    }

    #[test]
    fn a_recursive_generic_type_is_walked_to_its_end() {
        let list = Node::<Item> {
            item: Id::from_name("first"),
            next: Some(Box::new(Node {
                item: Id::from_name("second"),
                next: None,
                outcome: Ok(Id::from_name("unseen")),
            })),
            outcome: Err(Note(Id::from_name("unseen"))),
        };
        let mut to = Referenced::default();
        list.visit_references(&mut to);
        let fields: Vec<String> = to
            .into_found()
            .into_iter()
            .map(|found| found.field)
            .collect();
        assert_eq!(fields, ["item", "next.item"]);
    }

    /// A generic type with a field of a parameter's own type: that field
    /// isn't walked, unbound, the rest are, whatever the parameter.
    #[derive(crate::References)]
    struct Loot<T> {
        item: Id<Item>,
        payload: T,
    }

    #[test]
    fn a_parameters_field_leaves_the_rest_walked() {
        let loot = Loot {
            item: Id::from_name("pelt"),
            payload: Note(Id::from_name("unseen")),
        };
        let mut to = Referenced::default();
        to.enter("loot");
        // `Note` isn't `References`: `Loot<Note>` is, all the same.
        loot.visit_references(&mut to);
        to.leave();
        let found = to.into_found();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].field, "loot.item");
        assert_eq!(found[0].hash, Id::<Item>::from_name("pelt").hash());
    }

    /// A generic type whose parameter is bound by `References`.
    #[derive(crate::References)]
    struct Bound<T: References> {
        payload: T,
    }

    #[test]
    fn a_parameter_bound_by_references_is_walked() {
        let bound = Bound {
            payload: Find {
                tag: Id::from_name("fang"),
            },
        };
        let mut to = Referenced::default();
        bound.visit_references(&mut to);
        let found = to.into_found();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].field, "payload.tag");
    }

    #[test]
    fn a_nested_field_is_named_by_its_path_and_left_after() {
        let reward = (
            Find {
                tag: Id::from_name("fang"),
            },
            Id::<Item>::from_name("pelt"),
        );
        let mut to = Referenced::default();
        to.enter("reward");
        (&&Walk(&reward)).walk(&mut to);
        to.leave();
        let fields: Vec<String> = to
            .into_found()
            .into_iter()
            .map(|found| found.field)
            .collect();
        assert_eq!(fields, ["reward.tag", "reward"]);
    }

    /// A type of the app's own, without the derive: skipped.
    struct Note(#[expect(dead_code, reason = "only walked past")] Id<Item>);

    #[test]
    fn references_are_found_in_what_holds_them_and_only_there() {
        let pelt = Id::<Item>::from_name("pelt");
        let drops = vec![Some(pelt), None, Some(Id::from_name("fang"))];
        let mut to = Referenced::default();
        to.enter("drops");
        (&&Walk(&drops)).walk(&mut to);
        to.leave();
        to.enter("note");
        #[expect(clippy::needless_borrow, reason = "as the derives write it")]
        (&&Walk(&Note(pelt))).walk(&mut to);
        to.leave();
        to.enter("health");
        (&&Walk(&35_u32)).walk(&mut to);
        let table: HashMap<String, Vec<(Id<Item>, f32)>> =
            HashMap::from([("common".to_owned(), vec![(pelt, 0.5)])]);
        to.leave();
        to.enter("table");
        (&&Walk(&table)).walk(&mut to);
        let shared: Arc<[Id<Item>]> = Arc::from([pelt]);
        to.leave();
        to.enter("shared");
        (&&Walk(&shared)).walk(&mut to);
        let found = to.into_found();
        let fields: Vec<&str> = found.iter().map(|found| found.field.as_str()).collect();
        assert_eq!(fields, ["drops", "drops", "table", "shared"]);
        assert_eq!(found[0].field, "drops");
        assert_eq!(found[0].kind_name, "Item");
        assert_eq!(found[0].hash, pelt.hash());
    }
}
