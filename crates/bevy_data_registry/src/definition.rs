//! What a kind of definition is to the registry: [`Definition`], usually
//! derived; and [`Inherit`], how one of a kind with families fills itself
//! in from what it builds on.

use core::fmt::{Debug, Display};
use core::hash::Hash;

use serde::de::DeserializeOwned;

use crate::resolve::Resolve;

/// A kind of data definition: read from the data files its
/// [`resolvers`](Self::resolvers) claim, one definition or a list of them
/// per file, `(id: "sword", ..)`; and read through
/// [`Definitions`](crate::Definitions).
///
/// Derive it (see [the derive macro](derive@crate::Definition)) rather than
/// implementing it by hand, and add the kind to the app with
/// [`DataApp::register_definition`](crate::DataApp::register_definition).
///
/// A kind can have families: a definition then builds on another, its
/// [`base`](Self::base), and takes from it what it doesn't say itself
/// ([`inherit`](Self::inherit)). A [family](Self::is_family) itself is
/// only built on, never read.
pub trait Definition: DeserializeOwned + Send + Sync + 'static {
    /// Its name, in messages: `Item`.
    const NAME: &'static str;

    /// What tells definitions of this kind apart.
    type Id: Clone + Eq + Hash + Debug + Display + Send + Sync + 'static;

    /// The definition's id.
    fn id(&self) -> &Self::Id;

    /// Which data files are this kind's: those any of these claims. Called
    /// once, as the kind is registered. Derived from
    /// `#[definition(file = "..")]`, `folder`, `extension` and `resolve`.
    fn resolvers() -> Vec<Box<dyn Resolve>>;

    /// What it builds on, if anything: a family of its kind.
    fn base(&self) -> Option<&Self::Id> {
        None
    }

    /// Whether it's a family: only built on, never read.
    fn is_family(&self) -> bool {
        false
    }

    /// It, filled in from its `base`, itself already whole. `None` if the
    /// kind has no families, which makes a definition with a base an
    /// error.
    #[must_use]
    fn inherit(&self, base: &Self) -> Option<Self>
    where
        Self: Sized,
    {
        let _ = base;
        None
    }
}

/// How a definition of a kind with families fills in what it doesn't say
/// from its base. The derived [`Definition`] calls it on a clone of the
/// definition, with its base already whole.
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
