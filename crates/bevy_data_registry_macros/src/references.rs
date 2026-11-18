//! `#[derive(References)]`, and walking fields for the `Id`s in them: what
//! `#[derive(Definition)]` uses for its own `references`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Field, Fields, Ident, LitStr, Member, Path, Result};

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

/// Walks a type's fields for the `Id`s in them: what both derives write.
pub(crate) struct Walker<'a> {
    /// The path to `bevy_data_registry`.
    krate: &'a TokenStream,
}

impl<'a> Walker<'a> {
    /// A walker of a type's fields, `krate` the path to the crate.
    pub(crate) fn new(krate: &'a TokenStream) -> Self {
        Self { krate }
    }

    /// The `use` that walking needs in scope.
    pub(crate) fn walking(&self) -> TokenStream {
        let krate = self.krate;
        quote! {
            #[allow(unused_imports)]
            use #krate::__private::{ViaNothing as _, ViaReferences as _};
        }
    }

    /// Visits the `Id`s of `value`, a field named `name` (for messages), if
    /// its type is `References`; nothing if it's not.
    ///
    /// Picked by autoref, where the impl is written: in a generic type, by
    /// what's known of its parameters there. `Vec<Id<T>>` is walked, as
    /// it's `References` whatever `T` is; `payload: T` is only if `T` is
    /// bound by `References`.
    pub(crate) fn field(&self, name: &str, value: &TokenStream) -> TokenStream {
        let krate = self.krate;
        quote! {
            to.enter(#name);
            // Autoref picks `ViaReferences` if the field's type is
            // `References`, `ViaNothing` if not.
            #[allow(clippy::needless_borrow)]
            (&&#krate::__private::Walk(#value)).walk(to);
            to.leave();
        }
    }

    /// The walks of `fields`, each reached by `access` from its member;
    /// those marked `#[definition(unchecked)]` left out.
    fn fields(
        &self,
        fields: &Fields,
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
            let name = name.strip_prefix("r#").unwrap_or(&name);
            walks.push(self.field(name, &access(&member)));
        }
        Ok(walks)
    }
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
    let walker = Walker::new(&krate);
    let body = match &input.data {
        Data::Struct(data) => {
            let walks = walker.fields(&data.fields, |member| quote!(&self.#member))?;
            quote!(#(#walks)*)
        }
        // No value to visit: matched on what it points to, as `self`
        // itself, a reference, would need an arm.
        Data::Enum(data) if data.variants.is_empty() => quote!(match *self {}),
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
                let walks = walker.fields(&variant.fields, |member| {
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
    let walking = walker.walking();
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #krate::References for #name #type_generics #where_clause {
            fn visit_references(&self, to: &mut #krate::Referenced) {
                #walking
                #body
            }
        }
    })
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
        assert!(walked.contains(r#"to . enter ("item")"#), "{walked}");
        assert!(walked.contains(r#"to . enter ("chance")"#), "{walked}");
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
    fn a_generic_types_fields_are_walked_by_autoref_and_unbound() {
        let walked = expand(&parse_quote! {
            struct Drop<T: 'static> {
                items: Vec<Id<T>>,
                payload: T,
            }
        })
        .unwrap()
        .to_string();
        for field in ["items", "payload"] {
            assert!(
                walked.contains(&format!("Walk (& self . {field})")),
                "{field}: {walked}"
            );
        }
        assert!(!walked.contains("where"), "no bounds: {walked}");
    }

    #[test]
    fn an_enum_with_no_variants_visits_nothing() {
        let walked = expand(&parse_quote! { enum Never {} }).unwrap().to_string();
        assert!(walked.contains("match * self { }"), "{walked}");
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
