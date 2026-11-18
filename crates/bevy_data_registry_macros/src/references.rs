//! `#[derive(References)]`, and walking fields for the `Id`s in them: what
//! `#[derive(Definition)]` uses for its own `references`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::visit::Visit;
use syn::{Data, DeriveInput, Error, Field, Fields, Generics, Ident, LitStr, Member, Path, Result};

/// Whether `field` is marked `#[definition(unchecked)]`: its `Id`s aren't
/// checked. `others` says what else the field's `#[definition(..)]` may
/// say, for `#[derive(Definition)]`'s own; they're left to it.
pub(crate) fn unchecked(field: &Field, others: &[&str]) -> Result<bool> {
    let mut unchecked = false;
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("definition"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("unchecked") {
                unchecked = true;
                Ok(())
            } else if others.iter().any(|other| meta.path.is_ident(other)) {
                Ok(())
            } else {
                let mut expected: Vec<String> = others.iter().map(|o| format!("`{o}`")).collect();
                expected.push("`unchecked`".to_owned());
                Err(meta.error(format!("expected {}", expected.join(", "))))
            }
        })?;
    }
    Ok(unchecked)
}

/// Visits the `Id`s of `value`, a field named `name` (for messages) of type
/// `ty`: if its type is `References`, and skipped if not. A type naming one
/// of `generics`' parameters is skipped: whether it's `References` isn't
/// known where it's walked.
pub(crate) fn walk(
    krate: &TokenStream,
    name: &str,
    value: &TokenStream,
    ty: &syn::Type,
    generics: &Generics,
) -> Option<TokenStream> {
    if names_a_parameter(ty, generics) {
        return None;
    }
    Some(quote! {
        to.field(#name);
        // Autoref picks `ViaReferences` if the field's type is
        // `References`, `ViaNothing` if not.
        #[allow(clippy::needless_borrow)]
        (&&#krate::__private::Walk(#value)).walk(to);
    })
}

/// The `use` that walking needs in scope.
pub(crate) fn walking(krate: &TokenStream) -> TokenStream {
    quote! {
        #[allow(unused_imports)]
        use #krate::__private::{ViaNothing as _, ViaReferences as _};
    }
}

/// Whether `ty` names any of `generics`' type parameters.
fn names_a_parameter(ty: &syn::Type, generics: &Generics) -> bool {
    struct Finder<'a> {
        params: Vec<&'a Ident>,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Finder<'_> {
        fn visit_path(&mut self, path: &'ast Path) {
            if let Some(first) = path.segments.first()
                && self.params.iter().any(|param| first.ident == **param)
            {
                self.found = true;
            }
            syn::visit::visit_path(self, path);
        }
    }
    let mut finder = Finder {
        params: generics.type_params().map(|param| &param.ident).collect(),
        found: false,
    };
    if finder.params.is_empty() {
        return false;
    }
    finder.visit_type(ty);
    finder.found
}

/// The path to `bevy_data_registry`: `#[definition(crate = "..")]` on the
/// type, or `::bevy_data_registry`.
fn krate(input: &DeriveInput) -> Result<TokenStream> {
    let mut krate = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("definition"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                let value: LitStr = meta.value()?.parse()?;
                let path: Path = value.parse()?;
                krate = Some(quote!(#path));
                Ok(())
            } else {
                Err(meta.error("expected `crate = \"..\"`"))
            }
        })?;
    }
    Ok(krate.unwrap_or_else(|| quote!(::bevy_data_registry)))
}

pub(crate) fn expand(input: &DeriveInput) -> Result<TokenStream> {
    let krate = krate(input)?;
    let name = &input.ident;
    let generics = &input.generics;
    let body = match &input.data {
        Data::Struct(data) => {
            let walks = fields(
                &krate,
                &data.fields,
                generics,
                |member| quote!(&self.#member),
            )?;
            quote!(#(#walks)*)
        }
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                let variant_name = &variant.ident;
                let bindings: Vec<Ident> = (0..variant.fields.len())
                    .map(|at| format_ident!("field_{at}"))
                    .collect();
                let members = variant.fields.members();
                let pattern = if matches!(variant.fields, Fields::Unit) {
                    quote!(Self::#variant_name)
                } else {
                    quote!(Self::#variant_name { #(#members: #bindings),* })
                };
                let walks = fields(&krate, &variant.fields, generics, |member| {
                    let at = match member {
                        Member::Named(_) => variant
                            .fields
                            .members()
                            .position(|m| m == *member)
                            .unwrap_or_default(),
                        Member::Unnamed(index) => index.index as usize,
                    };
                    let binding = &bindings[at];
                    quote!(#binding)
                })?;
                arms.push(quote!(#pattern => { #(#walks)* }));
            }
            quote! {
                #[allow(unused_variables)]
                match self {
                    #(#arms)*
                }
            }
        }
        Data::Union(data) => {
            return Err(Error::new(
                data.union_token.span,
                "`References` is for structs and enums",
            ));
        }
    };
    let walking = walking(&krate);
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #krate::References for #name #type_generics #where_clause {
            fn visit_references(&self, to: &mut #krate::Referenced) {
                #walking
                #body
            }
        }
    })
}

/// The walks of `fields`, each reached by `access` from its member; those
/// marked `#[definition(unchecked)]` left out.
fn fields(
    krate: &TokenStream,
    fields: &Fields,
    generics: &Generics,
    access: impl Fn(&Member) -> TokenStream,
) -> Result<Vec<TokenStream>> {
    let mut walks = Vec::new();
    for (field, member) in fields.iter().zip(fields.members()) {
        if unchecked(field, &[])? {
            continue;
        }
        let name = match &member {
            Member::Named(ident) => ident.to_string(),
            Member::Unnamed(index) => index.index.to_string(),
        };
        let name = name.strip_prefix("r#").unwrap_or(&name).to_owned();
        walks.extend(walk(krate, &name, &access(&member), &field.ty, generics));
    }
    Ok(walks)
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    #[test]
    fn every_field_is_walked_but_the_unchecked() {
        let walked = expand(&parse_quote! {
            struct Drop {
                item: Id<Item>,
                #[definition(unchecked)]
                maybe: Id<Item>,
                chance: f32,
            }
        })
        .unwrap()
        .to_string();
        assert!(walked.contains(r#"to . field ("item")"#), "{walked}");
        assert!(walked.contains(r#"to . field ("chance")"#), "{walked}");
        assert!(!walked.contains(r#""maybe""#), "{walked}");
        assert!(walked.contains(":: bevy_data_registry :: References for Drop"));
    }

    #[test]
    fn an_enums_variants_are_matched() {
        let walked = expand(&parse_quote! {
            enum Reward {
                Item(Id<Item>),
                Gold { amount: u32 },
                Nothing,
            }
        })
        .unwrap()
        .to_string();
        assert!(walked.contains("Self :: Item { 0 : field_0 }"), "{walked}");
        assert!(
            walked.contains("Self :: Gold { amount : field_0 }"),
            "{walked}"
        );
        assert!(walked.contains("Self :: Nothing =>"), "{walked}");
    }

    #[test]
    fn a_field_of_a_parameters_type_is_skipped() {
        let walked = expand(&parse_quote! {
            struct Wrapper<T> { inner: T, items: Vec<Id<Item>> }
        })
        .unwrap()
        .to_string();
        assert!(!walked.contains(r#""inner""#), "{walked}");
        assert!(walked.contains(r#""items""#), "{walked}");
    }

    #[test]
    fn only_unchecked_is_understood_on_a_field() {
        let error = expand(&parse_quote! {
            struct Drop { #[definition(base)] item: Id<Item> }
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("expected `unchecked`"), "{error}");
    }
}
