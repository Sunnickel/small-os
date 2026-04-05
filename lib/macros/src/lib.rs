#![no_std]
#![no_main]

extern crate alloc;

use alloc::{format, string::ToString, vec::Vec};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    FnArg,
    Ident,
    ItemFn,
    LitStr,
    Pat,
    Token,
    Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

// ---------------------------------------------------------------------------
// CommandArgs — parses the #[command(name="..", short="..", long="..")] attr
// ---------------------------------------------------------------------------

struct CommandArgs {
    name: LitStr,
    short: LitStr,
    long: LitStr,
}

impl Parse for CommandArgs {
    fn parse(input: ParseStream) -> syn::Result<CommandArgs> {
        let mut name = None;
        let mut short = None;
        let mut long = None;
        let mut err: Option<syn::Error> = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            let _ = input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;

            match ident.to_string().as_str() {
                "name" => name = Some(value),
                "short" => short = Some(value),
                "long" => long = Some(value),
                _ => err = Some(syn::Error::new_spanned(ident, "unrecognized argument")),
            }

            let _ = input.parse::<Token![,]>();
        }

        if let Some(e) = err {
            return Err(e);
        }

        Ok(CommandArgs {
            name: name.expect("missing `name`"),
            short: short.expect("missing `short`"),
            long: long.expect("missing `long`"),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if `ty` is `Option<…>` (any path whose last segment is
/// `Option`).
fn is_option(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Option";
        }
    }
    false
}

/// Returns true if the param carries a `#[flag]` helper attribute.
fn has_flag_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("flag"))
}

/// Removes every `#[flag]` from an attribute list in-place.
fn strip_flag_attr(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|a| !a.path().is_ident("flag"));
}

// ---------------------------------------------------------------------------
// #[command]
// ---------------------------------------------------------------------------

/// Registers an async fn as a shell command.
///
/// Parameters are parsed from the function signature automatically:
///
/// ```rust
/// #[command(name = "echo", short = "Print text", long = "Prints text to screen")]
/// async fn cmd_echo(text: String, count: u32, #[flag] upper: bool) {
///     for _ in 0..count {
///         if upper {
///             println!("{}", text.to_uppercase());
///         } else {
///             println!("{}", text);
///         }
///     }
/// }
/// ```
///
/// Parameter rules:
///   - `ident: Type`           → required positional arg (parse fails = early
///     return)
///   - `ident: Option<Type>`   → optional positional arg (missing = None)
///   - `#[flag] ident: bool`   → `--ident` boolean flag, order-independent
///
/// The macro rewrites the signature to `(args: &[&str])` and injects
/// a parsing local for every parameter at the top of the body.
/// The old `#[arg]` attribute is no longer needed or exported.
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cmd_args = parse_macro_input!(attr as CommandArgs);
    let mut input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = input_fn.sig.ident.clone();
    let wrapper_name = format_ident!("{}_wrapper", fn_name);
    let static_name = format_ident!("{}_CMD_ENTRY", fn_name.to_string().to_uppercase());

    let name = &cmd_args.name;
    let short = &cmd_args.short;
    let long = &cmd_args.long;

    // ------------------------------------------------------------------
    // Walk the parameter list, classify each param, build parse stmts
    // ------------------------------------------------------------------
    let mut parse_stmts: Vec<syn::Stmt> = Vec::new();

    for param in &mut input_fn.sig.inputs {
        let FnArg::Typed(pt) = param else { continue };

        let Pat::Ident(pat_ident) = &*pt.pat else {
            continue;
        };

        let ident = &pat_ident.ident;
        let ty = &*pt.ty;
        let is_flag_param = has_flag_attr(&pt.attrs);

        strip_flag_attr(&mut pt.attrs);

        let stmt: syn::Stmt = if is_flag_param {
            let flag_str = format!("--{}", ident);
            syn::parse_quote! {
                let #ident: bool = __args.iter().any(|a| *a == #flag_str);
            }
        } else if is_option(ty) {
            syn::parse_quote! {
                let #ident: #ty =
                    __args_iter.next().and_then(|s| s.parse().ok());
            }
        } else {
            let name_str = ident.to_string();
            syn::parse_quote! {
                let #ident: #ty =
                    match __args_iter.next().and_then(|s| s.parse().ok()) {
                        Some(v) => v,
                        None => {
                            println!("Missing or invalid argument: {}", #name_str);
                            return;
                        }
                    };
            }
        };

        parse_stmts.push(stmt);
    }

    input_fn.sig.inputs = {
        let mut p: Punctuated<FnArg, Token![,]> = Punctuated::new();
        p.push(syn::parse_quote!(args: &[&str]));
        p
    };

    for stmt in parse_stmts.into_iter().rev() {
        input_fn.block.stmts.insert(0, stmt);
    }
    input_fn.block.stmts.insert(
        0,
        syn::parse_quote! {
            let mut __args_iter = args.iter().copied().filter(|a| !a.starts_with("--"));
        },
    );
    input_fn.block.stmts.insert(
        0,
        syn::parse_quote! {
            let __args: &[&str] = args;
        },
    );

    // ------------------------------------------------------------------
    // Emit rewritten function + wrapper + static command entry
    // ------------------------------------------------------------------
    let expanded = quote! {
        #input_fn

        fn #wrapper_name(args: &[&str])
            -> crate::task::shell::commands::CommandFuture<'static>
        {
            let owned: alloc::vec::Vec<&'static str> =
                unsafe { core::mem::transmute(args.to_vec()) };
            alloc::boxed::Box::pin(async move {
                #fn_name(owned.as_slice()).await
            })
        }

        #[used]
        #[unsafe(link_section = ".commands")]
        static #static_name: crate::task::shell::commands::CommandEntry =
            crate::task::shell::commands::CommandEntry {
                name:  #name,
                short: #short,
                long:  #long,
                args:  &[],
                func:  #wrapper_name,
            };
    };

    TokenStream::from(expanded)
}
