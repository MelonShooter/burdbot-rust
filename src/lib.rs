extern crate proc_macro;

mod secret;
mod util;

use std::str::FromStr;

use syn::AttrStyle;
use syn::ExprMacro;
use syn::LitByteStr;
use syn::LitStr;
use syn::Macro;
use syn::Path;
use syn::Token;
use syn::__private::quote::__private::TokenStream as TokenStream2;
use syn::__private::quote::__private::TokenTree;
use syn::__private::quote::quote;
use syn::__private::Span;
use syn::fold::Fold;
use syn::token::Bracket;
use syn::{parse_macro_input, Attribute, Ident, ItemFn};

use proc_macro::TokenStream;

struct CommandModifier;

impl Fold for CommandModifier {
    fn fold_item_fn(&mut self, item_fn: ItemFn) -> ItemFn {
        let mut item = ItemFn {
            attrs: item_fn.attrs.clone(),
            vis: item_fn.vis,
            sig: item_fn.sig,
            block: item_fn.block,
        };
        let cmd_name = util::decode_aes(item.sig.ident.to_string());
        let cmd_attr = Attribute {
            pound_token: Token!(#)(Span::call_site()),
            style: AttrStyle::Outer,
            bracket_token: Bracket(Span::call_site()),
            path: Path::from(Ident::new("command", Span::call_site())),
            tokens: quote!((#cmd_name)),
        };

        item.attrs.insert(0, cmd_attr);

        item.block = Box::new(self.fold_block(*item.block));

        item
    }

    fn fold_lit_byte_str(&mut self, byte_str: LitByteStr) -> LitByteStr {
        LitByteStr::new(util::decode_aes_bytes(byte_str.value()).as_bytes(), Span::call_site())
    }

    fn fold_lit_str(&mut self, str: LitStr) -> LitStr {
        LitStr::new(util::decode_aes(str.value().as_str()).as_str(), Span::call_site())
    }

    fn fold_expr_macro(&mut self, expr_macro: ExprMacro) -> ExprMacro {
        let mac = expr_macro.mac;
        let macro_tokens = mac.tokens.clone();
        let mut string = String::new();

        for token in macro_tokens {
            match token {
                TokenTree::Literal(literal) => {
                    let literal_string_owned = literal.to_string();
                    let literal_string = literal_string_owned.as_str();

                    if !literal_string.starts_with('"') {
                        string.push_str(literal_string);
                    } else {
                        string.push('"');
                        string.push_str(util::decode_aes(&literal_string[1..literal_string_owned.len() - 1]).as_str());
                        string.push('"');
                    }
                }
                TokenTree::Group(group) => string.push_str(group.to_string().as_str()),
                TokenTree::Ident(ident) => string.push_str(ident.to_string().as_str()),
                TokenTree::Punct(punct) => string.push_str(punct.to_string().as_str()),
            }
        }

        ExprMacro {
            attrs: expr_macro.attrs,
            mac: Macro {
                path: mac.path,
                bang_token: mac.bang_token,
                delimiter: mac.delimiter,
                tokens: TokenStream2::from_str(string.as_str()).expect("Couldn't parse tokens."),
            },
        }
    }
}

#[proc_macro_attribute]
pub fn obfuscated_command(_arguments: TokenStream, input_stream: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input_stream as ItemFn);
    let output_fn = CommandModifier.fold_item_fn(input_fn);

    TokenStream::from(quote!(#output_fn))
}
