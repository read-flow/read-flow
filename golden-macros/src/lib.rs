// SPDX-License-Identifier: GPL-3.0-or-later

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::Error;
use syn::ItemFn;
use syn::LitInt;
use syn::ReturnType;
use syn::Stmt;
use syn::Token;
use syn::parse::ParseStream;

/// Turns a function that returns a `cosmic::Element` into a golden image test.
///
/// The snapshot name is derived from the function name. Width and height (in pixels)
/// are required arguments. An optional third argument selects the theme:
/// `light` (default) or `dark`.
///
/// # Examples
///
/// ```rust,ignore
/// #[golden_test(320, 60)]
/// fn my_widget_light() -> cosmic::Element<'static, ()> {
///     my_widget().into()
/// }
///
/// #[golden_test(320, 60, dark)]
/// fn my_widget_dark() -> cosmic::Element<'static, ()> {
///     my_widget().into()
/// }
/// ```
///
/// Each expands to a `#[test]` that renders the element with the chosen theme and
/// calls `golden::assert_snapshot!`.
#[proc_macro_attribute]
pub fn golden_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    match golden_test_impl(attr, item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

struct Args {
    width: LitInt,
    height: LitInt,
    /// `None` means default (light).
    theme: Option<Ident>,
}

fn parse_args(input: ParseStream) -> syn::Result<Args> {
    let width: LitInt = input.parse()?;
    let _: Token![,] = input.parse()?;
    let height: LitInt = input.parse()?;
    let theme = if input.peek(Token![,]) {
        let _: Token![,] = input.parse()?;
        Some(input.parse::<Ident>()?)
    } else {
        None
    };
    Ok(Args {
        width,
        height,
        theme,
    })
}

fn golden_test_impl(attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let Args {
        width,
        height,
        theme,
    } = syn::parse::Parser::parse(parse_args, attr)?;

    let theme_expr = match theme.as_ref().map(|id| id.to_string()).as_deref() {
        Some("dark") => quote! { cosmic::Theme::dark() },
        Some("light") | None => quote! { cosmic::Theme::light() },
        Some(other) => {
            return Err(Error::new_spanned(
                theme.as_ref().unwrap(),
                format!("unknown theme '{other}': expected `dark` or `light`"),
            ));
        }
    };

    let func = syn::parse::<ItemFn>(item)?;

    if !func.sig.inputs.is_empty() {
        return Err(Error::new_spanned(
            &func.sig.inputs,
            "golden_test functions must take no parameters",
        ));
    }

    let func_name = &func.sig.ident;
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

    // Flatten the function body into the test body: emit all setup statements
    // as-is, then bind the tail expression to `element`.  This keeps any
    // locals (e.g. `let component = MyComponent::new()`) alive for the entire
    // test function, so elements that borrow them (via `component.view()`) do
    // not dangle.
    let stmts = &func.block.stmts;
    let (setup_stmts, element_expr) = match stmts.split_last() {
        Some((Stmt::Expr(expr, None), rest)) => (rest, expr),
        _ => {
            return Err(Error::new_spanned(
                &func.block,
                "golden_test function body must end with an expression (not a `;`-terminated statement)",
            ));
        }
    };

    Ok(quote! {
        #[test]
        fn #func_name() {
            #(#setup_stmts)*
            let element: #return_type = #element_expr;
            let mut renderer = golden::HeadlessRenderer::with_theme(#theme_expr);
            let rgba = renderer.render(element, #width, #height);
            golden::assert_snapshot_rgba!(#name_str, rgba, #width, #height);
        }
    }
    .into())
}
