//! [`Qualify`]: what the names read from data files stand for, before
//! they're hashed into ids.

use std::path::Path;

/// Turns a name as a data file writes it into the id's full name.
///
/// Set as [`DataRegistryPlugin::qualify`](crate::DataRegistryPlugin::qualify),
/// it's called for every [`Id`](crate::Id) read from a data file: a kind's
/// own id, its references and its base. The id is the full name's hash, and
/// messages, collisions and [`Definitions::name`](crate::Definitions::name)
/// show the full name. A closure of the same signature is one too.
///
/// ```rust
/// use bevy_data_registry::{Naming, Qualify};
///
/// /// `stone` is ours, `declaredlost:stone`; `coolmod:stone` is kept.
/// struct Namespaced;
///
/// impl Qualify for Namespaced {
///     fn qualify(
///         &self,
///         name: &str,
///         _: &Naming,
///     ) -> Result<Option<String>, String> {
///         match name.split_once(':') {
///             None => Ok(Some(format!("declaredlost:{name}"))),
///             Some((_, rest)) if rest.contains(':') => {
///                 Err("more than one `:`".into())
///             }
///             Some(_) => Ok(None),
///         }
///     }
/// }
///
/// let naming = Naming::new("blocks/ores.ron".as_ref(), "Block");
/// let full = Namespaced.qualify("stone", &naming);
/// assert_eq!(full, Ok(Some("declaredlost:stone".into())));
/// assert_eq!(Namespaced.qualify("coolmod:stone", &naming), Ok(None));
/// assert!(Namespaced.qualify("a:b:c", &naming).is_err());
/// ```
///
/// It must give the same answer for the same name and file, every time:
/// it's called on the threads files load on, may be called more than once
/// for one name (as serde reads some types twice: untagged enums,
/// flattened fields), and every app reading the same data must agree on
/// its ids. Ids made in code ([`Id::from_name`](crate::Id::from_name)) and
/// names deserialized outside a data file aren't qualified: code writes
/// the full name. Ids the rule reads itself are qualified too.
pub trait Qualify: Send + Sync + 'static {
    /// The full name of `name`, read where `naming` says: `None` if it's
    /// full as written.
    ///
    /// # Errors
    ///
    /// Why `name` isn't a valid one: its file fails to load, the error
    /// saying where in it, as for any other bad value. Inside an untagged
    /// enum, serde only says no variant matched.
    fn qualify(&self, name: &str, naming: &Naming) -> Result<Option<String>, String>;
}

impl<F> Qualify for F
where
    F: Fn(&str, &Naming) -> Result<Option<String>, String> + Send + Sync + 'static,
{
    fn qualify(&self, name: &str, naming: &Naming) -> Result<Option<String>, String> {
        self(name, naming)
    }
}

/// Where a name being [qualified](Qualify) was read.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Naming<'a> {
    /// The file's path in the data folder, as
    /// [`Resolve`](crate::resolve::Resolve) sees it:
    /// `enemies/spider/monsters.ron`.
    pub file: &'a Path,
    /// The kind it's an id of, as its type is named: `Monster`.
    pub kind: &'static str,
}

impl<'a> Naming<'a> {
    /// A name read in `file`, as an id of `kind`: to test a rule with.
    #[must_use]
    pub fn new(file: &'a Path, kind: &'static str) -> Self {
        Self { file, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closure_qualifies() {
        let rule: &dyn Qualify = &|name: &str, naming: &Naming| {
            Ok((!name.contains(':')).then(|| format!("{}:{name}", naming.kind)))
        };
        let naming = Naming::new(Path::new("tags.ron"), "Tag");
        assert_eq!(rule.qualify("fire", &naming), Ok(Some("Tag:fire".into())));
        assert_eq!(rule.qualify("mod:fire", &naming), Ok(None));
    }
}
