use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    LitInt, Token, parse::Parse, parse::ParseStream, parse_macro_input, punctuated::Punctuated,
};

struct DynamicSpacingInput {
    values: Punctuated<DynamicSpacingValue, Token![,]>,
}

// The input for the macro is a comma-separated list of values.
//
// When a single value is provided, the standard spacing formula is used to
// derive the compact/default/comfortable spacing values.
//
// When a tuple of three values is provided, the values are used as the
// compact/default/comfortable spacing values directly.
enum DynamicSpacingValue {
    Single(LitInt),
    Tuple(LitInt, LitInt, LitInt),
}

impl DynamicSpacingValue {
    /// The variant is named after the value used at the default UI density.
    fn variant_name(&self) -> syn::Result<syn::Ident> {
        let default_density_value = match self {
            DynamicSpacingValue::Single(n) => n,
            DynamicSpacingValue::Tuple(_, default, _) => default,
        };
        // `base10_parse` reports failures (e.g. a value that does not fit in
        // `u32`) with the span of the offending literal.
        let suffix = default_density_value.base10_parse::<u32>()?;
        Ok(format_ident!("Base{:02}", suffix))
    }

    fn doc_string(&self) -> syn::Result<String> {
        let (compact, default, comfortable) = match self {
            DynamicSpacingValue::Single(n) => {
                let n = n.base10_parse::<f32>()?;
                ((n - 4.0).max(0.0), n, n + 4.0)
            }
            DynamicSpacingValue::Tuple(compact, default, comfortable) => (
                compact.base10_parse::<f32>()?,
                default.base10_parse::<f32>()?,
                comfortable.base10_parse::<f32>()?,
            ),
        };
        Ok(format!(
            "`{compact}px`|`{default}px`|`{comfortable}px (@16px/rem)` - Scales with the user's rem size."
        ))
    }
}

impl Parse for DynamicSpacingInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(DynamicSpacingInput {
            values: input.parse_terminated(DynamicSpacingValue::parse, Token![,])?,
        })
    }
}

impl Parse for DynamicSpacingValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let a: LitInt = content.parse()?;
            content.parse::<Token![,]>()?;
            let b: LitInt = content.parse()?;
            content.parse::<Token![,]>()?;
            let c: LitInt = content.parse()?;
            Ok(DynamicSpacingValue::Tuple(a, b, c))
        } else {
            Ok(DynamicSpacingValue::Single(input.parse()?))
        }
    }
}

/// Derives the spacing method for the `DynamicSpacing` enum.
pub fn derive_spacing(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DynamicSpacingInput);
    match expand(&input) {
        Ok(expanded) => expanded,
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(input: &DynamicSpacingInput) -> syn::Result<TokenStream> {
    let spacing_ratios = input
        .values
        .iter()
        .map(|v| {
            let variant = v.variant_name()?;
            Ok(match v {
                DynamicSpacingValue::Single(n) => {
                    let n = n.base10_parse::<f32>()?;
                    quote! {
                        DynamicSpacing::#variant => match ::theme::theme_settings(cx).ui_density(cx) {
                            ::theme::UiDensity::Compact => (#n - 4.0).max(0.0) / BASE_REM_SIZE_IN_PX,
                            ::theme::UiDensity::Default => #n / BASE_REM_SIZE_IN_PX,
                            ::theme::UiDensity::Comfortable => (#n + 4.0) / BASE_REM_SIZE_IN_PX,
                        }
                    }
                }
                DynamicSpacingValue::Tuple(compact, default, comfortable) => {
                    let compact = compact.base10_parse::<f32>()?;
                    let default = default.base10_parse::<f32>()?;
                    let comfortable = comfortable.base10_parse::<f32>()?;
                    quote! {
                        DynamicSpacing::#variant => match ::theme::theme_settings(cx).ui_density(cx) {
                            ::theme::UiDensity::Compact => #compact / BASE_REM_SIZE_IN_PX,
                            ::theme::UiDensity::Default => #default / BASE_REM_SIZE_IN_PX,
                            ::theme::UiDensity::Comfortable => #comfortable / BASE_REM_SIZE_IN_PX,
                        }
                    }
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let variants_with_docs = input
        .values
        .iter()
        .map(|v| Ok((v.variant_name()?, v.doc_string()?)))
        .collect::<syn::Result<Vec<_>>>()?;
    let (variant_names, doc_strings): (Vec<_>, Vec<_>) = variants_with_docs
        .into_iter()
        .map(|(variant, doc_string)| (quote!(#variant), quote!(#doc_string)))
        .unzip();

    let expanded = quote! {
        /// A dynamic spacing system that adjusts spacing based on
        /// [UiDensity].
        ///
        /// The number following "Base" refers to the base pixel size
        /// at the default rem size and spacing settings.
        ///
        /// When possible, [DynamicSpacing] should be used over manual
        /// or built-in spacing values in places dynamic spacing is needed.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum DynamicSpacing {
            #(
                #[doc = #doc_strings]
                #variant_names,
            )*
        }

        impl DynamicSpacing {
            /// Returns the spacing ratio, should only be used internally.
            fn spacing_ratio(&self, cx: &App) -> f32 {
                const BASE_REM_SIZE_IN_PX: f32 = 16.0;
                match self {
                    #(#spacing_ratios,)*
                }
            }

            /// Returns the spacing value in rems.
            pub fn rems(&self, cx: &App) -> Rems {
                rems(self.spacing_ratio(cx))
            }

            /// Returns the spacing value in pixels.
            pub fn px(&self, cx: &App) -> Pixels {
                let ui_font_size_f32: f32 = ::theme::theme_settings(cx).ui_font_size(cx).into();
                px(ui_font_size_f32 * self.spacing_ratio(cx))
            }
        }
    };

    Ok(TokenStream::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dynamic_spacing_input_supports_single_and_tuple_values() {
        let input: DynamicSpacingInput = syn::parse_str("4, (2, 8, 12)").unwrap();

        assert_eq!(input.values.len(), 2);
        assert!(matches!(input.values[0], DynamicSpacingValue::Single(_)));
        assert!(matches!(
            input.values[1],
            DynamicSpacingValue::Tuple(_, _, _)
        ));
    }

    #[test]
    fn variant_name_uses_default_density_value() {
        let single: DynamicSpacingValue = syn::parse_str("4").unwrap();
        let tuple: DynamicSpacingValue = syn::parse_str("(2, 8, 12)").unwrap();

        assert_eq!(single.variant_name().unwrap().to_string(), "Base04");
        assert_eq!(tuple.variant_name().unwrap().to_string(), "Base08");
    }

    #[test]
    fn doc_string_formats_compact_default_and_comfortable_values() {
        let single: DynamicSpacingValue = syn::parse_str("2").unwrap();
        let tuple: DynamicSpacingValue = syn::parse_str("(1, 6, 9)").unwrap();

        assert_eq!(
            single.doc_string().unwrap(),
            "`0px`|`2px`|`6px (@16px/rem)` - Scales with the user's rem size."
        );
        assert_eq!(
            tuple.doc_string().unwrap(),
            "`1px`|`6px`|`9px (@16px/rem)` - Scales with the user's rem size."
        );
    }

    #[test]
    fn unsupported_spacing_value_reports_a_spanned_error() {
        let input: DynamicSpacingInput = syn::parse_str("99999999999").unwrap();

        let error = input.values[0].variant_name().unwrap_err();

        assert!(
            error.to_string().contains("number too large"),
            "unexpected error message: {error}"
        );
    }
}
