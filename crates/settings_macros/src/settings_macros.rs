use proc_macro::TokenStream;

use quote::quote;
use syn::{
    Data, DeriveInput, Field, Fields, ItemEnum, ItemStruct, Type, parse_macro_input, parse_quote,
};

/// Derives the `MergeFrom` trait for a struct.
///
/// This macro automatically implements `MergeFrom` by calling `merge_from`
/// on all fields in the struct.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone, MergeFrom)]
/// struct MySettings {
///     field1: Option<String>,
///     field2: SomeOtherSettings,
/// }
/// ```
#[proc_macro_derive(MergeFrom)]
pub fn derive_merge_from(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let merge_body = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => {
                let field_merges = fields.named.iter().map(|field| {
                    let field_name = &field.ident;
                    quote! {
                        self.#field_name.merge_from(&other.#field_name);
                    }
                });

                quote! {
                    #(#field_merges)*
                }
            }
            Fields::Unnamed(fields) => {
                let field_merges = fields.unnamed.iter().enumerate().map(|(i, _)| {
                    let field_index = syn::Index::from(i);
                    quote! {
                        self.#field_index.merge_from(&other.#field_index);
                    }
                });

                quote! {
                    #(#field_merges)*
                }
            }
            Fields::Unit => {
                quote! {
                    // No fields to merge for unit structs
                }
            }
        },
        Data::Enum(_) => {
            quote! {
                *self = other.clone();
            }
        }
        Data::Union(_) => {
            panic!("MergeFrom cannot be derived for unions");
        }
    };

    let expanded = quote! {
        impl #impl_generics crate::merge_from::MergeFrom for #name #ty_generics #where_clause {
            fn merge_from(&mut self, other: &Self) {
                use crate::merge_from::MergeFrom as _;
                #merge_body
            }
        }
    };

    TokenStream::from(expanded)
}

/// Registers the setting type with the SettingsStore. Note that you need to
/// have `gpui` in your dependencies for this to work.
#[proc_macro_derive(RegisterSetting)]
pub fn derive_register_setting(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let type_name = &input.ident;

    quote! {
        settings::private::inventory::submit! {
            settings::private::RegisteredSetting {
                settings_value: || {
                    Box::new(settings::private::SettingValue::<#type_name> {
                        global_value: None,
                        local_values: Vec::new(),
                    })
                },
                from_settings: |content| Box::new(<#type_name as settings::Settings>::from_settings(content)),
                id: || std::any::TypeId::of::<#type_name>(),
            }
        }
    }
    .into()
}

fn apply_on_fields(fields: &mut Fields) {
    match fields {
        Fields::Unit => {}
        Fields::Named(fields) => {
            for field in &mut fields.named {
                add_if_option(field)
            }
        }
        Fields::Unnamed(fields) => {
            for field in &mut fields.unnamed {
                add_if_option(field)
            }
        }
    }
}

fn add_if_option(field: &mut Field) {
    match &field.ty {
        Type::Path(syn::TypePath { qself: None, path })
            if path.leading_colon.is_none()
                && path.segments.len() == 1
                && path.segments[0].ident == "Option" => {}
        _ => return,
    }
    let attr = parse_quote!(
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with="crate::fallible_options::deserialize")]
    );
    field.attrs.push(attr);
}

// Adds serde attributes to each field with type Option<T>:
// #serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "settings::deserialize_fallible")
#[proc_macro_attribute]
pub fn with_fallible_options(_args: TokenStream, input: TokenStream) -> TokenStream {
    if let Ok(mut input) = syn::parse::<ItemStruct>(input.clone()) {
        apply_on_fields(&mut input.fields);
        quote!(#input).into()
    } else if let Ok(mut input) = syn::parse::<ItemEnum>(input) {
        for variant in &mut input.variants {
            apply_on_fields(&mut variant.fields);
        }
        quote!(#input).into()
    } else {
        panic!("with_fallible_options can only be applied to struct or enum definitions.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;
    use syn::parse_quote;

    #[test]
    fn add_if_option_marks_plain_option_fields() {
        let mut field: Field = parse_quote! {
            value: Option<String>
        };

        add_if_option(&mut field);

        assert_eq!(field.attrs.len(), 1);
        assert!(field.attrs[0].path().is_ident("serde"));
        let attr = field.attrs[0].meta.to_token_stream().to_string();
        assert!(attr.contains("skip_serializing_if"));
        assert!(attr.contains("deserialize_with"));
        assert!(attr.contains("fallible_options"));
    }

    #[test]
    fn add_if_option_ignores_non_matching_types() {
        let mut qualified_option: Field = parse_quote! {
            value: std::option::Option<String>
        };
        let mut non_option: Field = parse_quote! {
            value: String
        };

        add_if_option(&mut qualified_option);
        add_if_option(&mut non_option);

        assert!(qualified_option.attrs.is_empty());
        assert!(non_option.attrs.is_empty());
    }

    #[test]
    fn apply_on_fields_updates_named_and_unnamed_fields() {
        let mut named_struct: ItemStruct = parse_quote! {
            struct NamedFields {
                first: Option<String>,
                second: String,
            }
        };
        let mut tuple_struct: ItemStruct = parse_quote! {
            struct TupleFields(Option<String>, String);
        };

        apply_on_fields(&mut named_struct.fields);
        apply_on_fields(&mut tuple_struct.fields);

        let Fields::Named(named_fields) = named_struct.fields else {
            panic!("expected named fields");
        };
        let Fields::Unnamed(unnamed_fields) = tuple_struct.fields else {
            panic!("expected unnamed fields");
        };

        assert_eq!(named_fields.named[0].attrs.len(), 1);
        assert!(named_fields.named[1].attrs.is_empty());
        assert_eq!(unnamed_fields.unnamed[0].attrs.len(), 1);
        assert!(unnamed_fields.unnamed[1].attrs.is_empty());
    }
}
