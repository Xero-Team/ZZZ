use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    DeriveInput, Field, FieldsNamed, PredicateType, TraitBound, Type, TypeParamBound, WhereClause,
    WherePredicate, parse_macro_input, parse_quote,
};

#[proc_macro_derive(Refineable, attributes(refineable))]
pub fn derive_refineable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(expanded) => expanded,
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let generics = &input.generics;
    let attrs = &input.attrs;

    let refineable_attr = attrs.iter().find(|attr| attr.path().is_ident("refineable"));

    let mut impl_debug_on_refinement = false;
    let mut derives_serialize = false;
    let mut refinement_traits_to_derive = vec![];

    if let Some(refineable_attr) = refineable_attr {
        refineable_attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("Debug") {
                impl_debug_on_refinement = true;
            } else {
                if meta.path.is_ident("Serialize") {
                    derives_serialize = true;
                }
                refinement_traits_to_derive.push(meta.path);
            }
            Ok(())
        })?;
    }

    let refinement_ident = format_ident!("{}Refinement", ident);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(FieldsNamed { named, .. }),
            ..
        }) => named.iter().cloned().collect::<Vec<Field>>(),
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "Refineable can only be derived for structs with named fields",
            ));
        }
    };

    let field_names: Vec<_> = fields
        .iter()
        .map(|f| {
            f.ident
                .as_ref()
                .expect("named fields always have an identifier")
        })
        .collect();
    let field_visibilities: Vec<_> = fields.iter().map(|f| &f.vis).collect();
    let wrapped_types = fields
        .iter()
        .map(|f| get_wrapper_type(f, &f.ty))
        .collect::<syn::Result<Vec<_>>>()?;

    let field_attributes: Vec<TokenStream2> = fields
        .iter()
        .map(|f| {
            if derives_serialize {
                if is_refineable_field(f) {
                    quote! { #[serde(default, skip_serializing_if = "::refineable::IsEmpty::is_empty")] }
                } else {
                    quote! { #[serde(skip_serializing_if = "::std::option::Option::is_none")] }
                }
            } else {
                quote! {}
            }
        })
        .collect();

    // Create trait bound that each wrapped type must implement Clone
    let type_param_bounds: Vec<_> = wrapped_types
        .iter()
        .map(|ty| {
            WherePredicate::Type(PredicateType {
                attrs: Vec::new(),
                lifetimes: None,
                bounded_ty: ty.clone(),
                colon_token: Default::default(),
                bounds: {
                    let mut punctuated = syn::punctuated::Punctuated::new();
                    punctuated.push_value(TypeParamBound::Trait(TraitBound {
                        paren_token: None,
                        lifetimes: None,
                        modifiers: syn::TraitBoundModifiers::default(),
                        maybe: None,
                        path: parse_quote!(Clone),
                    }));

                    punctuated
                },
            })
        })
        .collect();

    // Append to where_clause or create a new one if it doesn't exist
    let where_clause = match where_clause.cloned() {
        Some(mut where_clause) => {
            where_clause.predicates.extend(type_param_bounds);
            where_clause
        }
        None => WhereClause {
            where_token: Default::default(),
            predicates: type_param_bounds.into_iter().collect(),
        },
    };

    let refineable_refine_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);
            let is_optional = is_optional_field(field);

            if is_refineable {
                quote! {
                    self.#name.refine(&refinement.#name);
                }
            } else if is_optional {
                quote! {
                    if let Some(value) = &refinement.#name {
                        self.#name = Some(value.clone());
                    }
                }
            } else {
                quote! {
                    if let Some(value) = &refinement.#name {
                        self.#name = value.clone();
                    }
                }
            }
        })
        .collect();

    let refineable_refined_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);
            let is_optional = is_optional_field(field);

            if is_refineable {
                quote! {
                    self.#name = self.#name.refined(refinement.#name);
                }
            } else if is_optional {
                quote! {
                    if let Some(value) = refinement.#name {
                        self.#name = Some(value);
                    }
                }
            } else {
                quote! {
                    if let Some(value) = refinement.#name {
                        self.#name = value;
                    }
                }
            }
        })
        .collect();

    let refinement_refine_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);

            if is_refineable {
                quote! {
                    self.#name.refine(&refinement.#name);
                }
            } else {
                quote! {
                    if let Some(value) = &refinement.#name {
                        self.#name = Some(value.clone());
                    }
                }
            }
        })
        .collect();

    let refinement_refined_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);

            if is_refineable {
                quote! {
                    self.#name = self.#name.refined(refinement.#name);
                }
            } else {
                quote! {
                    if let Some(value) = refinement.#name {
                        self.#name = Some(value);
                    }
                }
            }
        })
        .collect();

    let from_refinement_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);
            let is_optional = is_optional_field(field);

            if is_refineable {
                quote! {
                    #name: value.#name.into(),
                }
            } else if is_optional {
                quote! {
                    #name: value.#name.map(|v| v.into()),
                }
            } else {
                quote! {
                    #name: value.#name.map(|v| v.into()).unwrap_or_default(),
                }
            }
        })
        .collect();

    let debug_impl = if impl_debug_on_refinement {
        let refinement_field_debugs: Vec<TokenStream2> = fields
            .iter()
            .map(|field| {
                let name = &field.ident;
                quote! {
                    if self.#name.is_some() {
                        debug_struct.field(stringify!(#name), &self.#name);
                    } else {
                        all_some = false;
                    }
                }
            })
            .collect();

        quote! {
            impl #impl_generics std::fmt::Debug for #refinement_ident #ty_generics
                #where_clause
            {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let mut debug_struct = f.debug_struct(stringify!(#refinement_ident));
                    let mut all_some = true;
                    #( #refinement_field_debugs )*
                    if all_some {
                        debug_struct.finish()
                    } else {
                        debug_struct.finish_non_exhaustive()
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    let refinement_is_empty_conditions: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;

            if is_refineable_field(field) {
                quote! { self.#name.is_empty() }
            } else {
                quote! { self.#name.is_none() }
            }
        })
        .collect();

    let refineable_is_superset_conditions: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);
            let is_optional = is_optional_field(field);

            if is_refineable {
                quote! {
                    if !self.#name.is_superset_of(&refinement.#name) {
                        return false;
                    }
                }
            } else if is_optional {
                quote! {
                    if refinement.#name.is_some() && &self.#name != &refinement.#name {
                        return false;
                    }
                }
            } else {
                quote! {
                    if let Some(refinement_value) = &refinement.#name {
                        if &self.#name != refinement_value {
                            return false;
                        }
                    }
                }
            }
        })
        .collect();

    let refinement_is_superset_conditions: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);

            if is_refineable {
                quote! {
                    if !self.#name.is_superset_of(&refinement.#name) {
                        return false;
                    }
                }
            } else {
                quote! {
                    if refinement.#name.is_some() && &self.#name != &refinement.#name {
                        return false;
                    }
                }
            }
        })
        .collect();

    let refineable_subtract_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);
            let is_optional = is_optional_field(field);

            if is_refineable {
                quote! {
                    #name: self.#name.subtract(&refinement.#name),
                }
            } else if is_optional {
                quote! {
                    #name: if &self.#name == &refinement.#name {
                        None
                    } else {
                        self.#name.clone()
                    },
                }
            } else {
                quote! {
                    #name: if let Some(refinement_value) = &refinement.#name {
                        if &self.#name == refinement_value {
                            None
                        } else {
                            Some(self.#name.clone())
                        }
                    } else {
                        Some(self.#name.clone())
                    },
                }
            }
        })
        .collect();

    let refinement_subtract_assignments: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let name = &field.ident;
            let is_refineable = is_refineable_field(field);

            if is_refineable {
                quote! {
                    #name: self.#name.subtract(&refinement.#name),
                }
            } else {
                quote! {
                    #name: if &self.#name == &refinement.#name {
                        None
                    } else {
                        self.#name.clone()
                    },
                }
            }
        })
        .collect();

    let mut derive_stream = quote! {};
    for trait_to_derive in refinement_traits_to_derive {
        derive_stream.extend(quote! { #[derive(#trait_to_derive)] })
    }

    let r#gen = quote! {
        /// A refinable version of [`#ident`], see that documentation for details.
        #[derive(Clone)]
        #derive_stream
        pub struct #refinement_ident #impl_generics {
            #(
                #[allow(missing_docs)]
                #field_attributes
                #field_visibilities #field_names: #wrapped_types
            ),*
        }

        impl #impl_generics Refineable for #ident #ty_generics
            #where_clause
        {
            type Refinement = #refinement_ident #ty_generics;

            fn refine(&mut self, refinement: &Self::Refinement) {
                #( #refineable_refine_assignments )*
            }

            fn refined(mut self, refinement: Self::Refinement) -> Self {
                #( #refineable_refined_assignments )*
                self
            }

            fn is_superset_of(&self, refinement: &Self::Refinement) -> bool
            {
                #( #refineable_is_superset_conditions )*
                true
            }

            fn subtract(&self, refinement: &Self::Refinement) -> Self::Refinement
            {
                #refinement_ident {
                    #( #refineable_subtract_assignments )*
                }
            }
        }

        impl #impl_generics Refineable for #refinement_ident #ty_generics
            #where_clause
        {
            type Refinement = #refinement_ident #ty_generics;

            fn refine(&mut self, refinement: &Self::Refinement) {
                #( #refinement_refine_assignments )*
            }

            fn refined(mut self, refinement: Self::Refinement) -> Self {
                #( #refinement_refined_assignments )*
                self
            }

            fn is_superset_of(&self, refinement: &Self::Refinement) -> bool
            {
                #( #refinement_is_superset_conditions )*
                true
            }

            fn subtract(&self, refinement: &Self::Refinement) -> Self::Refinement
            {
                #refinement_ident {
                    #( #refinement_subtract_assignments )*
                }
            }
        }

        impl #impl_generics ::refineable::IsEmpty for #refinement_ident #ty_generics
            #where_clause
        {
            fn is_empty(&self) -> bool {
                true #( && #refinement_is_empty_conditions )*
            }
        }

        impl #impl_generics From<#refinement_ident #ty_generics> for #ident #ty_generics
            #where_clause
        {
            fn from(value: #refinement_ident #ty_generics) -> Self {
                Self {
                    #( #from_refinement_assignments )*
                }
            }
        }

        impl #impl_generics ::core::default::Default for #refinement_ident #ty_generics
            #where_clause
        {
            fn default() -> Self {
                #refinement_ident {
                    #( #field_names: Default::default() ),*
                }
            }
        }

        impl #impl_generics #refinement_ident #ty_generics
            #where_clause
        {
            /// Returns `true` if at least one field is `Some`, i.e. if this
            /// refinement carries any value at all.
            pub fn is_some(&self) -> bool {
                #(
                    if self.#field_names.is_some() {
                        return true;
                    }
                )*
                false
            }
        }

        #debug_impl
    };
    Ok(r#gen.into())
}

fn is_refineable_field(f: &Field) -> bool {
    f.attrs
        .iter()
        .any(|attr| attr.path().is_ident("refineable"))
}

fn is_optional_field(f: &Field) -> bool {
    if let Type::Path(typepath) = &f.ty
        && typepath.qself.is_none()
    {
        let segments = &typepath.path.segments;
        if segments.len() == 1 && segments.iter().any(|s| s.ident == "Option") {
            return true;
        }
    }
    false
}

fn get_wrapper_type(field: &Field, ty: &Type) -> syn::Result<syn::Type> {
    if is_refineable_field(field) {
        const NOT_A_STRUCT: &str = "a #[refineable] field must have a struct type";
        let Type::Path(type_path) = ty else {
            return Err(syn::Error::new_spanned(ty, NOT_A_STRUCT));
        };
        let Some(segment) = type_path.path.segments.last() else {
            return Err(syn::Error::new_spanned(ty, NOT_A_STRUCT));
        };

        let struct_name = &segment.ident;
        let refinement_struct_name = if struct_name.to_string().ends_with("Refinement") {
            format_ident!("{}", struct_name)
        } else {
            format_ident!("{}Refinement", struct_name)
        };
        let type_arguments = &segment.arguments;
        Ok(parse_quote!(#refinement_struct_name #type_arguments))
    } else if is_optional_field(field) {
        Ok(ty.clone())
    } else {
        Ok(parse_quote!(Option<#ty>))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;
    use syn::parse_quote;

    #[test]
    fn detects_refineable_fields_by_attribute() {
        let refineable_field: Field = parse_quote! {
            #[refineable]
            pub nested: Nested
        };
        let regular_field: Field = parse_quote! {
            pub nested: Nested
        };

        assert!(is_refineable_field(&refineable_field));
        assert!(!is_refineable_field(&regular_field));
    }

    #[test]
    fn detects_only_plain_option_fields() {
        let plain_option: Field = parse_quote! {
            pub value: Option<String>
        };
        let qualified_option: Field = parse_quote! {
            pub value: std::option::Option<String>
        };
        let regular_field: Field = parse_quote! {
            pub value: String
        };

        assert!(is_optional_field(&plain_option));
        assert!(!is_optional_field(&qualified_option));
        assert!(!is_optional_field(&regular_field));
    }

    #[test]
    fn get_wrapper_type_converts_refineable_and_plain_fields() {
        let refineable_field: Field = parse_quote! {
            #[refineable]
            pub nested: Nested<u8>
        };
        let optional_field: Field = parse_quote! {
            pub value: Option<String>
        };
        let plain_field: Field = parse_quote! {
            pub value: String
        };
        let existing_refinement_field: Field = parse_quote! {
            #[refineable]
            pub nested: NestedRefinement<u8>
        };

        assert_eq!(
            get_wrapper_type(&refineable_field, &refineable_field.ty)
                .unwrap()
                .into_token_stream()
                .to_string(),
            "NestedRefinement < u8 >"
        );
        assert_eq!(
            get_wrapper_type(&optional_field, &optional_field.ty)
                .unwrap()
                .into_token_stream()
                .to_string(),
            "Option < String >"
        );
        assert_eq!(
            get_wrapper_type(&plain_field, &plain_field.ty)
                .unwrap()
                .into_token_stream()
                .to_string(),
            "Option < String >"
        );
        assert_eq!(
            get_wrapper_type(&existing_refinement_field, &existing_refinement_field.ty)
                .unwrap()
                .into_token_stream()
                .to_string(),
            "NestedRefinement < u8 >"
        );
    }

    #[test]
    fn rejects_inputs_without_named_fields() {
        let tuple_struct: DeriveInput = parse_quote! {
            struct Tuple(u8);
        };
        let error = expand(&tuple_struct).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Refineable can only be derived for structs with named fields"
        );

        let enum_input: DeriveInput = parse_quote! {
            enum Choice {
                A,
            }
        };
        let error = expand(&enum_input).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Refineable can only be derived for structs with named fields"
        );
    }

    #[test]
    fn refineable_fields_must_be_struct_types() {
        let field: Field = parse_quote! {
            #[refineable]
            pub nested: &'static str
        };

        let error = get_wrapper_type(&field, &field.ty).unwrap_err();

        assert_eq!(
            error.to_string(),
            "a #[refineable] field must have a struct type"
        );
    }
}
