//! What a kind of definition is to the registry: [`Definition`], usually
//! derived; and [`Inherit`], how one of a kind with families fills itself
//! in from what it builds on.

use core::fmt::{Debug, Display};
use core::hash::Hash;

use serde::de::DeserializeOwned;

use crate::references::Referenced;
use crate::resolve::Resolve;

/// A kind of data definition, read from the data files its rules claim.
///
/// Derive it: the derive macro ([`Definition`](derive@crate::Definition))
/// sets which data files are the kind's, its id field, and, for a kind with
/// families, what a definition builds on and whether it's a family. Add the
/// kind to the app with
/// [`DataApp::register_definition`](crate::DataApp::register_definition),
/// and read it through [`Definitions`](crate::Definitions).
///
/// ```rust,ignore
/// #[derive(Definition, Deserialize)]
/// #[definition(file = "items")]
/// struct Item {
///     id: Id<Item>,
///     price: u32,
/// }
/// ```
///
/// Each file it claims holds one definition or a list of them,
/// `(id: "sword", ..)`. A kind can have families: a definition then builds
/// on another, and takes from it what it doesn't say itself ([`Inherit`]);
/// a family itself is only built on, never read.
pub trait Definition: DeserializeOwned + Send + Sync + 'static {
    /// What tells definitions of this kind apart: an [`Id`](crate::Id) of
    /// the kind, usually.
    type Id: Clone + Eq + Hash + Debug + Display + Send + Sync + 'static;

    /// The definition's id.
    fn id(&self) -> &Self::Id;

    /// What the crate needs to know of the kind: its name, its files, its
    /// references and its families. Called once, as it's registered.
    fn info() -> DefinitionInfo<Self>;
}

/// What the crate needs to know of a kind of [`Definition`], besides ids.
///
/// Built by the derive; by hand, start from [`new`](Self::new):
///
/// ```rust,ignore
/// DefinitionInfo::new("Item", vec![Box::new(FileName("items"))])
///     .references(|item, to| item.makes.visit_references(to))
///     .families(Families::new(|item| item.base.as_ref(), |item, base| ..))
/// ```
#[non_exhaustive]
pub struct DefinitionInfo<T: Definition> {
    /// The kind's name, in messages: `Item`.
    pub name: &'static str,
    /// Which data files are the kind's: those any of these claims.
    pub resolvers: Vec<Box<dyn Resolve>>,
    /// Visits every [`Id`](crate::Id) in a definition but its own and its
    /// base: those checked to name a definition.
    pub references: fn(&T, &mut Referenced),
    /// How a definition builds on another; `None` if the kind has no
    /// families.
    pub families: Option<Families<T>>,
}

impl<T: Definition> DefinitionInfo<T> {
    /// A kind named `name`, whose files `resolvers` claim: no references
    /// checked, no families.
    #[must_use]
    pub fn new(name: &'static str, resolvers: Vec<Box<dyn Resolve>>) -> Self {
        Self {
            name,
            resolvers,
            references: |_, _| {},
            families: None,
        }
    }

    /// Its references found by `references`.
    #[must_use]
    pub fn references(mut self, references: fn(&T, &mut Referenced)) -> Self {
        self.references = references;
        self
    }

    /// Its definitions building on others as `families` says.
    #[must_use]
    pub fn families(mut self, families: Families<T>) -> Self {
        self.families = Some(families);
        self
    }
}

/// How a definition of a kind with families builds on another.
#[non_exhaustive]
pub struct Families<T: Definition> {
    /// What it builds on, if anything: another definition of its kind.
    pub base: fn(&T) -> Option<&T::Id>,
    /// It, filled in from its base, itself already whole: a copy.
    pub inherit: fn(&T, &T) -> T,
    /// Whether it's a family: only built on, never read.
    pub is_family: fn(&T) -> bool,
}

impl<T: Definition> Families<T> {
    /// Families where a definition builds on what `base` says, filled in by
    /// `inherit`; none of them a family only built on.
    #[must_use]
    pub fn new(base: fn(&T) -> Option<&T::Id>, inherit: fn(&T, &T) -> T) -> Self {
        Self {
            base,
            inherit,
            is_family: |_| false,
        }
    }

    /// Those `is_family` says are families: only built on, never read.
    #[must_use]
    pub fn family(mut self, is_family: fn(&T) -> bool) -> Self {
        self.is_family = is_family;
        self
    }
}

impl<T: Definition> Clone for Families<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Definition> Copy for Families<T> {}

/// Fills in what a definition leaves out from its base, in kinds with
/// families.
///
/// Implement it for a kind with a `#[definition(base)]` field: the derive
/// calls it on a clone of the definition, with its base already whole.
///
/// ```rust,ignore
/// impl Inherit for Item {
///     fn inherit(&mut self, base: &Self) {
///         self.price = self.price.or(base.price);
///     }
/// }
/// ```
pub trait Inherit {
    /// Fills in what `self` leaves out from `base`.
    fn inherit(&mut self, base: &Self);
}
