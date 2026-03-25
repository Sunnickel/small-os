use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, LitStr};

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let name = parse_macro_input!(attr as LitStr);
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;

    let wrapper_name = format_ident!("{}_wrapper", fn_name);
    let static_name = format_ident!("{}_CMD_ENTRY", fn_name);

    let expanded = quote! {
        #input_fn

        fn #wrapper_name<'a>(args: &'a [&'a str])
            -> crate::task::shell::commands::CommandFuture<'a>
        {
            alloc::boxed::Box::pin(#fn_name(args))
        }

        #[used]
        #[link_section = ".commands"]
        static #static_name: crate::task::shell::commands::CommandEntry =
            crate::task::shell::commands::CommandEntry {
                name: #name,
                func: #wrapper_name,
            };
    };

    TokenStream::from(expanded)
}