extern crate proc_macro;

use std::str::FromStr;

use syn::AttrStyle;
use syn::Block;
use syn::Expr;
use syn::ExprCall;
use syn::ExprLit;
use syn::ExprPath;
use syn::Lit;
use syn::LitStr;
use syn::Path;
use syn::PathArguments;
use syn::PathSegment;
use syn::Stmt;
use syn::Token;
use syn::__private::quote::quote;
use syn::__private::Span;
use syn::__private::TokenStream2;
use syn::fold::Fold;
use syn::punctuated::Punctuated;
use syn::token::Bracket;
use syn::token::Paren;
use syn::{parse_macro_input, Attribute, Ident, ItemFn};

use proc_macro::TokenStream;

const INCORRECT_EXPR: &str = "The function body should only contain one string literal without a semicolon.";

struct CommandModifier;

impl Fold for CommandModifier {
    fn fold_item_fn(&mut self, item_fn: ItemFn) -> ItemFn {
        let mut item = ItemFn { attrs: item_fn.attrs.clone(), vis: item_fn.vis, sig: item_fn.sig, block: item_fn.block };
        let cmd_name = burdbot_macros_internal::decode_aes(item.sig.ident.to_string());
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

    fn fold_block(&mut self, block: Block) -> Block {
        let statements = block.stmts;

        if statements.len() != 1 {
            panic!("{}", INCORRECT_EXPR);
        }

        let first_statement = statements.first().unwrap().to_owned();

        Block { brace_token: block.brace_token, stmts: vec![self.fold_stmt(first_statement)] }
    }

    fn fold_stmt(&mut self, statement: Stmt) -> Stmt {
        match statement {
            Stmt::Expr(expr) => Stmt::Expr(self.fold_expr(expr)),
            _ => panic!("{}", INCORRECT_EXPR),
        }
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Lit(lit) => Expr::Lit(self.fold_expr_lit(lit)),
            _ => panic!("{}", INCORRECT_EXPR),
        }
    }

    fn fold_expr_lit(&mut self, expr_lit: ExprLit) -> ExprLit {
        ExprLit { attrs: (expr_lit.attrs), lit: (self.fold_lit(expr_lit.lit)) }
    }

    fn fold_lit(&mut self, lit: Lit) -> Lit {
        match lit {
            Lit::Str(lit_str) => Lit::Str(self.fold_lit_str(lit_str)),
            _ => panic!("{}", INCORRECT_EXPR),
        }
    }

    fn fold_lit_str(&mut self, str: LitStr) -> LitStr {
        LitStr::new(burdbot_macros_internal::decode_aes(str.value().as_str()).as_str(), Span::call_site())
    }
}

#[proc_macro_attribute]
pub fn obfuscated_command(_arguments: TokenStream, input_stream: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input_stream as ItemFn);
    let mut output_fn = CommandModifier.fold_item_fn(input_fn);
    let statement = output_fn.block.stmts.first().unwrap().clone();
    let mut value = String::with_capacity(128);
    let mut code = "";

    if let Stmt::Expr(Expr::Lit(expr_lit)) = statement {
        if let Lit::Str(lit) = expr_lit.lit {
            value.push('{');
            value.push_str(lit.value().as_str());
            value.push('}');

            code = value.as_str();
        }
    }

    if code.is_empty() {
        panic!("Code was never set. This should never ever happen.");
    }

    let body = TokenStream::from(TokenStream2::from_str(code).unwrap());
    output_fn.block = Box::new(parse_macro_input!(body as Block));

    TokenStream::from(quote!(#output_fn))
}

fn create_segment(str: &str) -> PathSegment {
    PathSegment { ident: Ident::new(str, Span::call_site()), arguments: PathArguments::None }
}

struct Encoder;

impl Fold for Encoder {
    fn fold_lit_str(&mut self, lit_str: LitStr) -> LitStr {
        LitStr::new(burdbot_macros_internal::encode_aes(lit_str.value()).as_str(), Span::call_site())
    }
}

#[proc_macro]
pub fn aes_encode_decode(tokens: TokenStream) -> TokenStream {
    let mut mac = parse_macro_input!(tokens as LitStr);
    mac = Encoder.fold_lit_str(mac);

    let lit = Expr::Lit(ExprLit { attrs: Vec::new(), lit: Lit::Str(mac) });
    let mut func_call = Punctuated::new();

    func_call.push(create_segment("burdbot_macros"));
    func_call.push(create_segment("decode_aes"));

    let mut args = Punctuated::new();

    args.push(lit);

    let path = Path { leading_colon: None, segments: func_call };
    let expr = ExprCall {
        attrs: Vec::new(),
        func: Box::new(Expr::Path(ExprPath { attrs: Vec::new(), qself: None, path })),
        paren_token: Paren(Span::call_site()),
        args,
    };

    TokenStream::from(quote!(#expr))
}
