//! Which data files are a kind's: [`Resolve`], and the resolvers that come
//! with the crate, [`FileName`], [`Folder`] and [`Extension`].

use core::fmt;
use std::path::Path;

/// A rule for which data files hold a kind's definitions, chosen with
/// `#[definition(file = "..")]`, `folder`, `extension`, or
/// `resolve = ..` for any other.
///
/// A kind's files are those any of its rules claims. Every data file must
/// be claimed by exactly one kind: one no kind claims, or two kinds do, is
/// an error, never settled by precedence. Two kinds with the same rule (as
/// displayed) panic when the second is registered.
///
/// Displayed in those errors, it says what it claims: "files named
/// `items`".
///
/// ```rust,ignore
/// /// Files at the data folder's top only, by name.
/// struct TopLevel(&'static str);
///
/// impl Resolve for TopLevel {
///     fn claims(&self, path: &Path) -> bool {
///         path.parent() == Some(Path::new("")) && FileName(self.0).claims(path)
///     }
/// }
///
/// impl fmt::Display for TopLevel {
///     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
///         write!(f, "files named `{}` at the top", self.0)
///     }
/// }
/// ```
pub trait Resolve: fmt::Display + Send + Sync + 'static {
    /// Whether the file at `path`, relative to the data folder
    /// (`enemies/spider/items.ron`), is this kind's.
    fn claims(&self, path: &Path) -> bool;
}

/// Claims every file with this name, in any folder: `FileName("items")`
/// claims `items.ron` and `enemies/spider/items.ron`, whatever its format.
/// A name is the file's up to its first dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileName(pub &'static str);

impl Resolve for FileName {
    fn claims(&self, path: &Path) -> bool {
        name_parts(path).is_some_and(|parts| parts.first() == Some(&self.0))
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "files named `{}`", self.0)
    }
}

/// Claims every file in this folder, at any depth, from the data folder's
/// top: `Folder("skills")` claims `skills/fire.ron` and
/// `skills/rare/void.ron`, not `enemies/skills/bite.ron`. Can be several
/// folders deep: `Folder("affixes/prefixes")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Folder(pub &'static str);

impl Resolve for Folder {
    fn claims(&self, path: &Path) -> bool {
        path.parent()
            .is_some_and(|parent| parent.starts_with(self.0))
    }
}

impl fmt::Display for Folder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "files under `{}/`", self.0)
    }
}

/// Claims every file whose name ends in this, before its format's
/// extension, in any folder: `Extension("items")` claims
/// `swords.items.ron` and `spider/loot.items.json`, not `items.ron`
/// (a [`FileName`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extension(pub &'static str);

impl Resolve for Extension {
    fn claims(&self, path: &Path) -> bool {
        name_parts(path).is_some_and(|parts| parts.len() >= 3 && parts[parts.len() - 2] == self.0)
    }
}

impl fmt::Display for Extension {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "files ending `.{}.<format>`", self.0)
    }
}

/// The file's name, split at its dots: `swords.items.ron` is `swords`,
/// `items`, `ron`.
fn name_parts(path: &Path) -> Option<Vec<&str>> {
    Some(path.file_name()?.to_str()?.split('.').collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(rule: &dyn Resolve, path: &str) -> bool {
        rule.claims(Path::new(path))
    }

    #[test]
    fn a_file_name_is_claimed_in_any_folder_and_format() {
        let items = FileName("items");
        assert!(claims(&items, "items.ron"));
        assert!(claims(&items, "enemies/spider/items.json"));
        assert!(claims(&items, "items.data.ron"), "up to the first dot");
        assert!(!claims(&items, "items/swords.ron"), "a folder isn't a file");
        assert!(!claims(&items, "rare_items.ron"));
        assert!(!claims(&items, "swords.items.ron"), "that's an extension");
    }

    #[test]
    fn a_folder_is_claimed_at_any_depth_from_the_top() {
        let skills = Folder("skills");
        assert!(claims(&skills, "skills/fire.ron"));
        assert!(claims(&skills, "skills/rare/void.ron"));
        assert!(
            !claims(&skills, "enemies/skills/bite.ron"),
            "only from the top"
        );
        assert!(!claims(&skills, "skills.ron"), "a file isn't a folder");
        assert!(!claims(&skills, "skillsets/a.ron"), "whole folder names");
        let prefixes = Folder("affixes/prefixes");
        assert!(claims(&prefixes, "affixes/prefixes/damage.ron"));
        assert!(!claims(&prefixes, "affixes/suffixes/speed.ron"));
    }

    #[test]
    fn an_extension_is_claimed_before_the_formats() {
        let items = Extension("items");
        assert!(claims(&items, "swords.items.ron"));
        assert!(claims(&items, "enemies/spider/loot.items.json"));
        assert!(claims(&items, "a.b.items.ron"));
        assert!(!claims(&items, "items.ron"), "that's a file name");
        assert!(!claims(&items, "swords.items"), "no format");
    }

    #[test]
    fn a_rule_says_what_it_claims() {
        assert_eq!(FileName("items").to_string(), "files named `items`");
        assert_eq!(Folder("skills").to_string(), "files under `skills/`");
        assert_eq!(
            Extension("items").to_string(),
            "files ending `.items.<format>`"
        );
    }
}
