//! What a kind of definition is to the registry: where its files are and
//! what tells its definitions apart ([`Definition`]); and, for a kind with
//! families, how one builds on another ([`FamilyDefinition`]).

use core::fmt::Display;
use core::hash::Hash;

use serde::de::DeserializeOwned;

/// A kind of data definition, loaded from the files its
/// [`FOLDER`](Self::FOLDER) names: those in a folder of that name, and
/// those named after it anywhere (see [`kind_of`](crate::kind_of)).
pub trait Definition: DeserializeOwned + Send + Sync + 'static {
    /// What holds this kind under the definitions' root: a folder of this
    /// name, or a file named after it (`items.ron`), anywhere: `items`.
    const FOLDER: &'static str;

    /// Its name, singular, in messages: `item`.
    const NAME: &'static str;

    /// What tells definitions of this kind apart.
    type Id: Clone + Eq + Hash + Display + Send + Sync + 'static;

    /// The definition's id.
    fn id(&self) -> &Self::Id;
}

/// A kind whose definitions can build on a family: an abstract definition,
/// never read, whose whole they take what they don't say from. Made whole
/// as they're indexed, so what [`Defs`](crate::Defs) reads is whole.
pub trait FamilyDefinition: Definition {
    /// What it builds on, if anything.
    fn base(&self) -> Option<&Self::Id>;

    /// Whether it's a family, only built on: [`Defs`](crate::Defs) never
    /// shows it.
    fn family(&self) -> bool;

    /// It, filled in from its whole `base`.
    #[must_use]
    fn inherit(&self, base: &Self) -> Self;
}
