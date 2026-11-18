//! `#[derive(References)]`, and walking fields for the `Id`s in them: what
//! `#[derive(Definition)]` uses for its own `references`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::visit::Visit;
use syn::{
    Data, DeriveInput, Error, Field, Fields, Generics, Ident, LitStr, Member, Path, Result, Type,
};

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
    /// The type's own name, if the impl being written is its `References`:
    /// a field holding the type itself is then one too.
    owner: Option<&'a Ident>,
    /// The type's type parameters.
    params: Vec<&'a Ident>,
}

impl<'a> Walker<'a> {
    /// A walker of the fields of a type with `generics`, for the impl
    /// written: its `References` if `owner`, its name, is given.
    pub(crate) fn new(
        krate: &'a TokenStream,
        generics: &'a Generics,
        owner: Option<&'a Ident>,
    ) -> Self {
        Self {
            krate,
            owner,
            params: generics.type_params().map(|param| &param.ident).collect(),
        }
    }

    /// The `use` that walking needs in scope.
    pub(crate) fn walking(&self) -> TokenStream {
        let krate = self.krate;
        quote! {
            #[allow(unused_imports)]
            use #krate::__private::{ViaNothing as _, ViaReferences as _};
        }
    }

    /// Visits the `Id`s of `value`, a field named `name` (for messages) of
    /// type `ty`, if its type is `References`; nothing if it's not, or if
    /// that can't be known for every value of the type's parameters.
    ///
    /// A type naming none of them is walked by autoref: `References` if it
    /// is. One naming them only inside `Id<..>` (`Vec<Id<T>>`), or inside
    /// the type itself where its `References` is written, is `References`
    /// whatever they are: visited directly. One naming them elsewhere
    /// (`payload: T`) isn't walked: bounding the impl by it would leave the
    /// type no `References` at all where it isn't one, its other `Id`s
    /// unchecked with it.
    pub(crate) fn field(&self, name: &str, value: &TokenStream, ty: &Type) -> Option<TokenStream> {
        let krate = self.krate;
        let walk = match self.parameters_in(ty) {
            Parameters::None => quote! {
                // Autoref picks `ViaReferences` if the field's type is
                // `References`, `ViaNothing` if not.
                #[allow(clippy::needless_borrow)]
                (&&#krate::__private::Walk(#value)).walk(to);
            },
            Parameters::InIds => quote! {
                #krate::References::visit_references(#value, to);
            },
            Parameters::Elsewhere => return None,
        };
        Some(quote! {
            to.enter(#name);
            #walk
            to.leave();
        })
    }

    /// The walks of `fields`, each reached by `access` from its member;
    /// those marked `#[definition(unchecked)]`, or that can't be walked,
    /// left out.
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
            walks.extend(self.field(name, &access(&member), &field.ty));
        }
        Ok(walks)
    }

    /// Where `ty` names the type's parameters.
    fn parameters_in(&self, ty: &Type) -> Parameters {
        struct Finder<'a, 'b> {
            walker: &'b Walker<'a>,
            named: bool,
            elsewhere: bool,
        }
        impl<'ast> Visit<'ast> for Finder<'_, '_> {
            fn visit_path(&mut self, path: &'ast Path) {
                let is = |ident: &Ident, name: &Ident| ident == name;
                if let Some(first) = path.segments.first()
                    && self
                        .walker
                        .params
                        .iter()
                        .any(|param| is(&first.ident, param))
                {
                    self.named = true;
                    self.elsewhere = true;
                    return;
                }
                // `References` whatever its arguments: not looked into.
                let safe = path.segments.last().is_some_and(|last| {
                    last.ident == "Id"
                        || self.walker.owner.is_some_and(|owner| last.ident == *owner)
                        || (self.walker.owner.is_some() && last.ident == "Self")
                });
                if safe {
                    let mut inside = Finder {
                        walker: self.walker,
                        named: false,
                        elsewhere: false,
                    };
                    syn::visit::visit_path(&mut inside, path);
                    self.named |= inside.named;
                    return;
                }
                syn::visit::visit_path(self, path);
            }
        }
        if self.params.is_empty() {
            return Parameters::None;
        }
        let mut finder = Finder {
            walker: self,
            named: false,
            elsewhere: false,
        };
        finder.visit_type(ty);
        match (finder.named, finder.elsewhere) {
            (false, _) => Parameters::None,
            (true, false) => Parameters::InIds,
            (true, true) => Parameters::Elsewhere,
        }
    }
}

/// Where a field's type names the type's parameters.
enum Parameters {
    /// Nowhere.
    None,
    /// Only inside what's `References` whatever they are.
    InIds,
    /// Elsewhere too.
    Elsewhere,
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
    let walker = Walker::new(&krate, &input.generics, Some(name));
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
    fn a_parameter_inside_an_id_is_walked_and_elsewhere_isnt() {
        let walked = expand(&parse_quote! {
            struct Drop<T: 'static> {
                item: Id<T>,
                items: Vec<Id<T>>,
                next: Option<Box<Drop<T>>>,
                payload: T,
                tagged: Vec<Id<Item>>,
            }
        })
        .unwrap()
        .to_string();
        for direct in ["item", "items", "next"] {
            assert!(
                walked.contains(&format!(
                    "References :: visit_references (& self . {direct} , to)"
                )),
                "{direct}: {walked}"
            );
        }
        assert!(!walked.contains(r#""payload""#), "{walked}");
        assert!(walked.contains(r#""tagged""#), "{walked}");
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
