//! [`Id`]: a definition's id, typed by its kind, and a reference to one
//! from another; and the names ids are read from, kept per file for
//! messages.

use core::any::TypeId;
use core::cell::RefCell;
use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};

/// The id of a definition of kind `T`, or a reference to one from another.
///
/// Written as its name in data files (`"sword"`), held as a 64-bit hash of
/// it.
/// `T` is the kind it's an id of, not what it's made of: an `Id<Item>`
/// can't be passed where an `Id<Monster>` is wanted. It's [`Copy`], and
/// compared and hashed as a number. It shows as its hash (`#3f2a…`): the
/// name it was read from is kept per data file, for the crate's messages,
/// and [`Definitions::name`](crate::Definitions::name) finds it.
///
/// ```rust,ignore
/// #[derive(Definition, Deserialize)]
/// #[definition(file = "monsters")]
/// struct Monster {
///     id: Id<Monster>,
///     drops: Vec<Id<Item>>, // each checked to be an item
/// }
///
/// const PELT: Id<Item> = Id::from_name("pelt");
/// let pelt = items.get(&PELT);
/// ```
///
/// Every `Id` in a definition, but its own, its base and those in fields
/// marked `#[definition(unchecked)]`, is checked once every kind is
/// indexed, as one of [`DataSystems::Validate`](crate::DataSystems::Validate):
/// it must name a definition [`Definitions::get`](crate::Definitions::get)
/// finds, not nothing and not a family (families are only built on). What's
/// checked is what `get` gives: whole, or kept from before a reload. Two
/// names of a kind with the same hash are an error too: one needs
/// renaming.
///
/// A kind's own id field, if it's an `Id`, must be one of the kind: the
/// derive checks it, by the type as written (`Id<..>`). Another type named
/// `Id` as an id field needs a path (`my::Id<..>`) not ending `Id<..>`, or
/// an alias; an alias of an `Id` of another kind isn't caught, and its
/// references fail the check at run time instead.
///
/// ```compile_fail,E0308
/// # use bevy_data_registry::prelude::*;
/// # use serde::Deserialize;
/// struct Monster;
///
/// #[derive(Definition, Deserialize)]
/// #[definition(file = "items")]
/// struct Item {
///     id: Id<Monster>, // not an `Id<Item>`
/// }
/// ```
///
/// It's a [`Component`], to say which definition an entity is made from.
#[derive(Component)]
pub struct Id<T: 'static> {
    hash: u64,
    kind: PhantomData<fn() -> T>,
}

impl<T: 'static> Id<T> {
    /// The id named `name`: its 64-bit FNV-1a hash.
    #[must_use]
    pub const fn from_name(name: &str) -> Self {
        let bytes = name.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut at = 0;
        while at < bytes.len() {
            hash ^= bytes[at] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            at += 1;
        }
        Self::from_hash(hash)
    }

    /// The id with this hash.
    #[must_use]
    pub const fn from_hash(hash: u64) -> Self {
        Self {
            hash,
            kind: PhantomData,
        }
    }

    /// Its hash.
    #[must_use]
    pub const fn hash(self) -> u64 {
        self.hash
    }
}

impl<T: 'static> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Id<T> {}

impl<T: 'static> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<T: 'static> Eq for Id<T> {}

impl<T: 'static> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: 'static> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hash.cmp(&other.hash)
    }
}

impl<T: 'static> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl<T: 'static> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{:016x}", self.hash)
    }
}

impl<T: 'static> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Id<{}>({self})", short_type_name::<T>())
    }
}

impl<'de, T: 'static> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(Named(PhantomData))
    }
}

/// Reads an [`Id`] from its name: hashed where it lies, not copied.
struct Named<T: 'static>(PhantomData<fn() -> T>);

impl<T: 'static> Visitor<'_> for Named<T> {
    type Value = Id<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "the name of a {}", short_type_name::<T>())
    }

    fn visit_str<E: de::Error>(self, name: &str) -> Result<Id<T>, E> {
        let id = Id::from_name(name);
        NAMES.with_borrow_mut(|names| {
            if let Some(names) = names {
                names.keep(TypeId::of::<T>(), id.hash, name);
            }
        });
        Ok(id)
    }
}

/// `T`'s name, without its path: `Item`.
pub(crate) fn short_type_name<T: ?Sized>() -> &'static str {
    let name = core::any::type_name::<T>();
    let name = name.split('<').next().unwrap_or(name);
    name.rsplit("::").next().unwrap_or(name)
}

/// The names [`Id`]s were read from, by kind and hash: those of one data
/// file, as it's read.
#[derive(Debug, Default)]
pub(crate) struct Names {
    /// The first name read of each id.
    first: HashMap<(TypeId, u64), Box<str>>,
    /// Every other name read with the same kind and hash: a collision.
    others: Vec<(TypeId, u64, Box<str>)>,
}

impl Names {
    /// Keeps `name`, of an id of `kind` with `hash`, unless it's kept.
    pub(crate) fn keep(&mut self, kind: TypeId, hash: u64, name: &str) {
        match self.first.get(&(kind, hash)) {
            None => {
                self.first.insert((kind, hash), name.into());
            }
            Some(first) if **first == *name => {}
            Some(_) => {
                let kept = self
                    .others
                    .iter()
                    .any(|(k, h, n)| *k == kind && *h == hash && **n == *name);
                if !kept {
                    self.others.push((kind, hash, name.into()));
                }
            }
        }
    }

    /// The name of the id of `kind` with `hash`, if one was read.
    pub fn get(&self, kind: TypeId, hash: u64) -> Option<&str> {
        self.first.get(&(kind, hash)).map(AsRef::as_ref)
    }

    /// The id of `kind` with `hash` for messages: its name if one was
    /// read, as an [`Id`] shows if not.
    pub fn show(&self, kind: TypeId, hash: u64) -> String {
        self.get(kind, hash)
            .map_or_else(|| format!("#{hash:016x}"), ToOwned::to_owned)
    }

    /// Every name of an id of `kind`, with its hash: those colliding too.
    pub fn of(&self, kind: TypeId) -> impl Iterator<Item = (u64, &str)> {
        let first = self
            .first
            .iter()
            .filter(move |((k, _), _)| *k == kind)
            .map(|((_, hash), name)| (*hash, &**name));
        let others = self
            .others
            .iter()
            .filter(move |(k, _, _)| *k == kind)
            .map(|(_, hash, name)| (*hash, &**name));
        first.chain(others)
    }
}

thread_local! {
    /// The names of the ids read on this thread, while a file is.
    static NAMES: RefCell<Option<Names>> = const { RefCell::new(None) };
}

/// Runs `read`, and returns with what it returns the names of every [`Id`]
/// it read on this thread: a format must read on the thread it's called
/// on. If `read` panics, the names kept before are kept again.
pub(crate) fn naming<R>(read: impl FnOnce() -> R) -> (R, Names) {
    /// Puts back what was kept before, however `read` ends.
    struct Restore(Option<Names>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            NAMES.with_borrow_mut(|names| *names = outer);
        }
    }

    let restore = Restore(NAMES.with_borrow_mut(|names| names.replace(Names::default())));
    let result = read();
    let names = NAMES.with_borrow_mut(Option::take).unwrap_or_default();
    drop(restore);
    (result, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Item;
    struct Monster;

    #[test]
    fn an_id_is_its_names_hash() {
        const SWORD: Id<Item> = Id::from_name("sword");
        assert_eq!(SWORD, Id::from_name("sword"));
        assert_ne!(SWORD, Id::from_name("swords"));
        // FNV-1a's, as published.
        assert_eq!(Id::<Item>::from_name("").hash(), 0xcbf2_9ce4_8422_2325);
        assert_eq!(Id::<Item>::from_name("a").hash(), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(Id::<Item>::from_hash(SWORD.hash()), SWORD);
        let read: Id<Item> = serde_json::from_str(r#""sword""#).unwrap();
        assert_eq!(read, SWORD);
        assert_eq!(SWORD.to_string(), format!("#{:016x}", SWORD.hash()));
        assert!(format!("{SWORD:?}").starts_with("Id<Item>(#"));
    }

    #[test]
    fn the_names_read_are_kept_while_naming() {
        let (ids, names) = naming(|| {
            serde_json::from_str::<Vec<Id<Item>>>(r#"["pelt", "fang", "pelt"]"#).unwrap()
        });
        assert_eq!(ids.len(), 3);
        let item = TypeId::of::<Item>();
        assert_eq!(names.get(item, ids[0].hash()), Some("pelt"));
        assert_eq!(names.get(item, ids[1].hash()), Some("fang"));
        assert_eq!(names.of(item).count(), 2, "once each");
        assert_eq!(names.get(TypeId::of::<Monster>(), ids[0].hash()), None);
        let ((), outside) = naming(|| ());
        assert_eq!(outside.of(item).count(), 0);
    }

    #[test]
    fn a_name_colliding_in_a_file_is_kept_too() {
        let mut names = Names::default();
        let item = TypeId::of::<Item>();
        names.keep(item, 7, "sword");
        names.keep(item, 7, "sword");
        names.keep(item, 7, "axe");
        assert_eq!(names.get(item, 7), Some("sword"), "the first");
        let all: Vec<&str> = names.of(item).map(|(_, name)| name).collect();
        assert_eq!(all, ["sword", "axe"]);
        assert_eq!(names.show(item, 8), "#0000000000000008");
    }

    #[test]
    fn a_panic_while_naming_leaves_nothing_named() {
        let caught = std::panic::catch_unwind(|| {
            naming(|| panic!("a format gave up"));
        });
        assert!(caught.is_err());
        NAMES.with_borrow(|names| assert!(names.is_none(), "restored"));
        let borrowed: Id<Item> = serde_json::from_str(r#""sword""#).unwrap();
        assert_eq!(borrowed, Id::from_name("sword"), "read outside naming");
    }
}
