//! `#[derive(Definition)]`: reading the struct's `#[definition(..)]`
//! attributes, and writing its `Definition` impl.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Expr, Field, Fields, Ident, LitStr, Path, Result};

use crate::references::{self, Walker};

/// What the attributes say about the struct's fields.
struct Marked<'a> {
    id: Option<&'a Field>,
    base: Option<&'a Field>,
    family: Option<&'a Field>,
}

pub(crate) fn expand(input: &DeriveInput) -> Result<TokenStream> {
    let name = &input.ident;
    let Options { resolvers, krate } = options(input)?;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => return Err(Error::new(name.span(), NAMED_ONLY)),
        },
        Data::Enum(data) => return Err(Error::new(data.enum_token.span, NAMED_ONLY)),
        Data::Union(data) => return Err(Error::new(data.union_token.span, NAMED_ONLY)),
    };

    let mut marked = Marked {
        id: None,
        base: None,
        family: None,
    };
    for field in fields {
        mark(field, &mut marked)?;
    }
    let id = marked
        .id
        .or_else(|| {
            fields
                .iter()
                .find(|field| field.ident.as_ref().is_some_and(|ident| ident == "id"))
        })
        .ok_or_else(|| {
            Error::new(
                name.span(),
                "a definition needs an id: a field named `id`, or one marked `#[definition(id)]`",
            )
        })?;
    if let (Some(family), None) = (marked.family, marked.base) {
        return Err(Error::new(
            family.span(),
            "families are only built on: mark the field a definition builds on with `#[definition(base)]`",
        ));
    }

    let id_ident = &id.ident;
    let id_type = &id.ty;
    let family = marked.family.map(|family| {
        let family = &family.ident;
        quote!(.family(|def| def.#family))
    });
    let families = marked.base.map(|base| {
        let base = &base.ident;
        quote! {
            .families(
                #krate::Families::<Self>::new(
                    |def| ::core::option::Option::as_ref(&def.#base),
                    |def, base| {
                        let mut this = ::core::clone::Clone::clone(def);
                        #krate::Inherit::inherit(&mut this, base);
                        this
                    },
                )
                #family
            )
        }
    });

    let kind = name.to_string();
    let kind = kind.strip_prefix("r#").unwrap_or(&kind);
    if resolvers.is_empty() {
        return Err(Error::new(
            name.span(),
            "say which data files are this kind's: `#[definition(file = \"..\")]`, `folder = \"..\"`, `extension = \"..\"` or `resolve = ..`",
        ));
    }
    let resolvers = resolvers.iter().map(|resolver| resolver.tokens(&krate));
    let id_check = is_an_id(id_type).then(|| {
        quote! {
            // Its id is an id of its own kind.
            let _: &#krate::Id<Self> = &self.#id_ident;
        }
    });
    // Its `References` isn't what's written: a field holding the kind
    // itself isn't one.
    let walker = Walker::new(&krate);
    let references = references_fn(&walker, fields, id, marked.base)?;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #krate::Definition for #name #type_generics #where_clause {
            type Id = #id_type;

            fn id(&self) -> &Self::Id {
                #id_check
                &self.#id_ident
            }

            fn info() -> #krate::DefinitionInfo<Self> {
                #krate::DefinitionInfo::<Self>::new(
                    #kind,
                    ::std::vec![#(::std::boxed::Box::new(#resolvers)),*],
                )
                .references(#references)
                #families
            }
        }
    })
}

const NAMED_ONLY: &str = "a definition is a struct with named fields";

/// The closure finding a definition's references: walking every field but
/// the id, the base (checked as families are made whole) and those marked
/// `#[definition(unchecked)]`.
fn references_fn<'a>(
    walker: &Walker,
    fields: impl IntoIterator<Item = &'a Field>,
    id: &Field,
    base: Option<&Field>,
) -> Result<TokenStream> {
    let mut walks = Vec::new();
    for field in fields.into_iter().filter(|field| {
        !core::ptr::eq(*field, id) && !base.is_some_and(|base| core::ptr::eq(*field, base))
    }) {
        if references::unchecked(field, &["id", "base", "family"])? {
            continue;
        }
        let Some(ident) = &field.ident else {
            continue;
        };
        let name = ident.to_string();
        let name = name.strip_prefix("r#").unwrap_or(&name);
        walks.push(walker.field(name, &quote!(&def.#ident)));
    }
    let walking = walker.walking();
    Ok(quote! {
        |def, to| {
            let _ = (&def, &to);
            #walking
            #(#walks)*
        }
    })
}

/// Whether `ty` is written as an `Id<..>`: checked to be the kind's own.
fn is_an_id(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    path.path.segments.last().is_some_and(|last| {
        last.ident == "Id" && matches!(last.arguments, syn::PathArguments::AngleBracketed(_))
    })
}

/// What the struct's own `#[definition(..)]` says.
struct Options {
    /// Which data files are its: any of these claims.
    resolvers: Vec<Resolver>,
    /// `crate = ".."`: the path to `bevy_data_registry`, if it's renamed or
    /// re-exported; `::bevy_data_registry` if not.
    krate: TokenStream,
}

/// One rule for which files are the kind's.
enum Resolver {
    /// `file = "items"`, `folder = "skills"` or `extension = "items"`: a
    /// resolver of the crate's, by its type's name.
    BuiltIn(&'static str, LitStr),
    /// `resolve = ..`: any value of a type implementing `Resolve`.
    Custom(Box<Expr>),
}

impl Resolver {
    /// The resolver, as an expression.
    fn tokens(&self, krate: &TokenStream) -> TokenStream {
        match self {
            Self::BuiltIn(kind, value) => {
                let kind = Ident::new(kind, value.span());
                quote!(#krate::resolve::#kind(#value))
            }
            Self::Custom(expr) => quote!(#expr),
        }
    }
}

/// Reads the struct's `#[definition(..)]`: `file`, `folder`, `extension`
/// and `resolve`, any number of each; `crate`, once.
fn options(input: &DeriveInput) -> Result<Options> {
    let mut resolvers = Vec::new();
    let mut krate = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("definition"))
    {
        attr.parse_nested_meta(|meta| {
            let built_in = [
                ("file", "FileName"),
                ("folder", "Folder"),
                ("extension", "Extension"),
            ]
            .into_iter()
            .find(|(key, _)| meta.path.is_ident(key));
            if let Some((key, kind)) = built_in {
                let value: LitStr = meta.value()?.parse()?;
                check(key, &value)?;
                resolvers.push(Resolver::BuiltIn(kind, value));
                Ok(())
            } else if meta.path.is_ident("resolve") {
                resolvers.push(Resolver::Custom(Box::new(meta.value()?.parse()?)));
                Ok(())
            } else if meta.path.is_ident("crate") {
                if krate.is_some() {
                    return Err(meta.error("the crate is given twice"));
                }
                let value: LitStr = meta.value()?.parse()?;
                let path: Path = value.parse()?;
                krate = Some(quote!(#path));
                Ok(())
            } else {
                Err(meta.error(
                    "expected `file = \"..\"`, `folder = \"..\"`, `extension = \"..\"`, `resolve = ..` or `crate = \"..\"`",
                ))
            }
        })?;
    }
    Ok(Options {
        resolvers,
        krate: krate.unwrap_or_else(|| quote!(::bevy_data_registry)),
    })
}

/// Checks a built-in resolver's value: a name for `file` and `extension`,
/// a path from the data folder's top for `folder`.
fn check(key: &str, value: &LitStr) -> Result<()> {
    let text = value.value();
    let fine = if key == "folder" {
        !text.is_empty()
            && !text.contains('\\')
            && text
                .split('/')
                // `:` would make `C:` a drive on Windows: never in the data folder.
                .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(':'))
    } else {
        !text.is_empty() && !text.contains(['/', '\\', '.'])
    };
    if fine {
        return Ok(());
    }
    let what = if key == "folder" {
        "a folder from the data folder's top, `/` between its parts: `skills`, `affixes/prefixes`"
    } else {
        "a name without dots or slashes: `items`"
    };
    Err(Error::new(value.span(), format!("`{key}` takes {what}")))
}

/// Notes what `field`'s `#[definition(..)]` marks it as.
fn mark<'a>(field: &'a Field, marked: &mut Marked<'a>) -> Result<()> {
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("definition"))
    {
        attr.parse_nested_meta(|meta| {
            let slot = if meta.path.is_ident("id") {
                &mut marked.id
            } else if meta.path.is_ident("base") {
                &mut marked.base
            } else if meta.path.is_ident("family") {
                &mut marked.family
            } else if meta.path.is_ident("unchecked") {
                // For `references`.
                return Ok(());
            } else {
                return Err(meta.error("expected `id`, `base`, `family` or `unchecked`"));
            };
            if slot.is_some() {
                let what = meta
                    .path
                    .get_ident()
                    .map_or_else(String::new, Ident::to_string);
                return Err(meta.error(format!("only one field can be marked `{what}`")));
            }
            *slot = Some(field);
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    fn error(input: &DeriveInput) -> String {
        expand(input).unwrap_err().to_string()
    }

    #[test]
    fn the_name_is_the_types() {
        let item = expand(&parse_quote! {
            #[definition(file = "items")]
            struct Item { id: String }
        })
        .unwrap();
        assert!(
            item.to_string()
                .contains(r#"DefinitionInfo :: < Self > :: new ("Item""#),
            "{item}"
        );
    }

    #[test]
    fn every_rule_claims_its_files() {
        let item = expand(&parse_quote! {
            #[definition(file = "items", folder = "affixes/items")]
            #[definition(extension = "items", resolve = TopLevel("loot"))]
            struct Item { id: String }
        })
        .unwrap()
        .to_string();
        for rule in [
            r#":: bevy_data_registry :: resolve :: FileName ("items")"#,
            r#":: bevy_data_registry :: resolve :: Folder ("affixes/items")"#,
            r#":: bevy_data_registry :: resolve :: Extension ("items")"#,
            r#"Box :: new (TopLevel ("loot"))"#,
        ] {
            assert!(item.contains(rule), "{rule} in {item}");
        }
    }

    #[test]
    fn a_kind_must_claim_files_and_rules_must_make_sense() {
        let none = error(&parse_quote! { struct Item { id: String } });
        assert!(none.contains("say which data files"), "{none}");
        for (rule, says) in [
            (quote!(file = "items.ron"), "without dots"),
            (quote!(extension = "a/b"), "without dots"),
            (quote!(file = ""), "without dots"),
            (quote!(folder = "/skills"), "from the data folder's top"),
            (quote!(folder = "skills/../x"), "from the data folder's top"),
            (quote!(folder = "skills/"), "from the data folder's top"),
            (quote!(folder = "C:/mods"), "from the data folder's top"),
        ] {
            let input: DeriveInput = parse_quote! {
                #[definition(#rule)]
                struct Item { id: String }
            };
            let error = error(&input);
            assert!(error.contains(says), "{rule}: {error}");
        }
    }

    #[test]
    fn a_resolver_can_be_any_expression() {
        let item = expand(&parse_quote! {
            #[definition(resolve = { let name = "loot"; TopLevel(name) })]
            struct Item { id: String }
        })
        .unwrap()
        .to_string();
        assert!(item.contains("let name"), "{item}");
    }

    #[test]
    fn an_id_field_is_checked_to_be_the_kinds_and_others_walked() {
        let item = expand(&parse_quote! {
            #[definition(file = "monsters")]
            struct Monster {
                #[definition(base)]
                base: Option<Id<Monster>>,
                id: Id<Monster>,
                drops: Vec<Id<Item>>,
                #[definition(unchecked)]
                rumoured: Vec<Id<Item>>,
                health: u32,
            }
        })
        .unwrap()
        .to_string();
        assert!(
            item.contains("let _ : & :: bevy_data_registry :: Id < Self > = & self . id"),
            "{item}"
        );
        assert!(item.contains(r#"to . enter ("drops")"#), "{item}");
        assert!(item.contains(r#"to . enter ("health")"#), "{item}");
        assert!(!item.contains(r#""rumoured""#), "unchecked: {item}");
        assert!(!item.contains(r#"to . enter ("id")"#), "its own: {item}");
        assert!(
            !item.contains(r#"to . enter ("base")"#),
            "checked by families: {item}"
        );
        let named = expand(&parse_quote! {
            #[definition(file = "items")]
            struct Item { id: String }
        })
        .unwrap()
        .to_string();
        assert!(!named.contains("Id < Self >"), "not an `Id`: {named}");
    }

    #[test]
    fn the_crate_can_be_named_where_its_renamed() {
        let default = expand(&parse_quote! {
            #[definition(file = "items")]
            struct Item { id: String }
        })
        .unwrap();
        assert!(
            default
                .to_string()
                .contains(":: bevy_data_registry :: Definition")
        );
        let renamed = expand(&parse_quote! {
            #[definition(crate = "engine::data", file = "items")]
            struct Item { id: String }
        })
        .unwrap()
        .to_string();
        assert!(
            renamed.contains("engine :: data :: Definition"),
            "{renamed}"
        );
        assert!(!renamed.contains("bevy_data_registry"), "{renamed}");
    }

    #[test]
    fn a_base_brings_inheriting_and_a_family_needs_one() {
        let families = expand(&parse_quote! {
            #[definition(file = "items")]
            struct Item {
                #[definition(id)] name: String,
                #[definition(base)] parent: Option<String>,
                #[definition(family)] family: bool,
            }
        })
        .unwrap()
        .to_string();
        assert!(
            families.contains(". families (:: bevy_data_registry :: Families :: < Self > :: new")
                && families.contains("Option :: as_ref (& def . parent)")
                && families.contains("Inherit :: inherit"),
            "{families}"
        );
        assert!(
            families.contains(". family (| def | def . family)")
                && families.contains("self . name"),
            "{families}"
        );
        let lone = error(&parse_quote! {
            struct Item { id: String, #[definition(family)] family: bool }
        });
        assert!(lone.contains("#[definition(base)]"), "{lone}");
    }

    #[test]
    fn what_cant_be_a_definition_says_why() {
        assert!(
            error(&parse_quote! { #[definition(file = "items")] struct Item { name: String } })
                .contains("needs an id")
        );
        assert!(error(&parse_quote! { struct Item(String); }).contains("named fields"));
        assert!(error(&parse_quote! { enum Item { A } }).contains("named fields"));
        assert!(
            error(&parse_quote! {
                struct Item { #[definition(id)] a: String, #[definition(id)] b: String }
            })
            .contains("only one field")
        );
        assert!(
            error(&parse_quote! { struct Item { #[definition(key)] id: String } })
                .contains("expected `id`, `base`, `family` or `unchecked`")
        );
    }
}
