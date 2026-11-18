#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod definition;

use proc_macro::TokenStream;
use syn::{DeriveInput, Error, parse_macro_input};

/// Derives `Definition`: the kind's name, which data files are its, its
/// id, and, for a kind with families, its base and whether a definition is
/// a family.
///
/// ```rust,ignore
/// #[derive(Definition, Deserialize, Clone)]
/// #[definition(file = "items", folder = "items")] // at least one rule
/// struct Item {
///     id: String, // or any field marked #[definition(id)]
///     #[definition(base)]
///     base: Option<String>,
///     #[definition(family)]
///     #[serde(default)]
///     family: bool,
///     price: Option<u32>,
/// }
/// ```
///
/// Its files are those any of its rules claims: `file = "items"` (every
/// file named `items`), `folder = "weapons"` (every file under `weapons/`),
/// `extension = "items"` (every `*.items.<format>`), or `resolve = ..`
/// (any value of a type implementing `Resolve`); any number of each, at
/// least one.
///
/// `#[definition(crate = "engine::data")]` names `bevy_data_registry`
/// where it's renamed or re-exported.
///
/// A struct with named fields only. Its id is the field marked
/// `#[definition(id)]`, or else the one named `id`. A `#[definition(base)]`
/// field, an `Option` of the id, makes it a kind with families: it must
/// then be `Clone` and implement `Inherit`. A `#[definition(family)]`
/// field, a `bool`, marks the families, which are only built on.
#[proc_macro_derive(Definition, attributes(definition))]
pub fn derive_definition(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    definition::expand(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
