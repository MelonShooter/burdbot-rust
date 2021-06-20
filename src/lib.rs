extern crate proc_macro;

mod secret;
mod util;

use syn::AttrStyle;
use syn::Path;
use syn::Token;
use syn::__private::quote::quote;
use syn::__private::Span;
use syn::fold::Fold;
use syn::token::Bracket;
use syn::{parse_macro_input, Attribute, Ident, ItemFn};

use proc_macro::TokenStream;

struct CommandModifier;

impl Fold for CommandModifier {
    fn fold_item_fn(&mut self, item_fn: ItemFn) -> ItemFn {
        let mut item = item_fn.clone();
        let cmd_name = util::decode_aes(item.sig.ident.to_string());
        let cmd_attr = Attribute {
            pound_token: Token!(#)(Span::call_site()),
            style: AttrStyle::Outer,
            bracket_token: Bracket(Span::call_site()),
            path: Path::from(Ident::new("command", Span::call_site())),
            tokens: quote!((#cmd_name)),
        };

        item.attrs.insert(0, cmd_attr);

        item
    }
}

#[proc_macro_attribute]
pub fn obfuscated_command(_arguments: TokenStream, input_stream: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input_stream as ItemFn);
    let output_fn = CommandModifier.fold_item_fn(input_fn);

    TokenStream::from(quote!(#output_fn))
}
