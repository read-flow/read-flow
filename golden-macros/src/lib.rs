// SPDX-License-Identifier: GPL-3.0-or-later

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::Error;
use syn::ItemFn;
use syn::LitInt;
use syn::ReturnType;
use syn::Token;
use syn::parse::Parser;
use syn::punctuated::Punctuated;

/// Turns a function that returns a `cosmic::Element` into a golden image test.
///
/// The snapshot name is derived from the function name. Width and height (in pixels)
/// are required arguments.
///
/// # Example
///
/// ```rust,ignore
/// #[golden_test(320, 60)]
/// fn text_hello_world() -> cosmic::Element<'static, ()> {
///     cosmic::widget::text("Hello, world!").into()
/// }
/// ```
///
/// This expands to a `#[test]` function that renders the element and calls
/// `golden::assert_snapshot!("text_hello_world", element, 320, 60)`.
#[proc_macro_attribute]
pub fn golden_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    match golden_test_impl(attr, item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

fn golden_test_impl(attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let args = Punctuated::<LitInt, Token![,]>::parse_terminated.parse(attr)?;

    let mut it = args.iter();
    let width = it.next().ok_or_else(|| {
        Error::new(
            Span::call_site(),
            "expected width and height: #[golden_test(width, height)]",
        )
    })?;
    let height = it.next().ok_or_else(|| {
        Error::new(
            Span::call_site(),
            "expected height: #[golden_test(width, height)]",
        )
    })?;

    let func = syn::parse::<ItemFn>(item)?;

    if !func.sig.inputs.is_empty() {
        return Err(Error::new_spanned(
            &func.sig.inputs,
            "golden_test functions must take no parameters",
        ));
    }

    let func_name = &func.sig.ident;
    let func_body = &func.block;
    let name_str = func_name.to_string();

    let return_type = match &func.sig.output {
        ReturnType::Type(_, ty) => ty.as_ref(),
        ReturnType::Default => {
            return Err(Error::new_spanned(
                &func.sig,
                "golden_test functions must have an explicit return type",
            ));
        }
    };

    Ok(quote! {
        #[test]
        fn #func_name() {
            let element: #return_type = { #func_body };
            golden::assert_snapshot!(#name_str, element, #width, #height);
        }
    }
    .into())
}
