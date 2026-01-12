// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::manual_unwrap_or_default)] // `FromMeta` currently generates clippy warnings

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, AttributeArgs, ItemFn};

#[derive(Debug, FromMeta)]
struct QueryArgs {
    #[darling(default)]
    cache: bool,
    #[darling(default)]
    delegate: bool,
    #[darling(default)]
    spawn: bool,
}

#[proc_macro_attribute]
pub fn query(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);
    let mut fun = parse_macro_input!(input as ItemFn);

    let args = match QueryArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(e.write_errors()),
    };

    if !args.cache {
        basic_query(&mut fun, args.delegate, args.spawn);
    } else if fun.sig.inputs.is_empty() {
        global_query(&mut fun, args.delegate, args.spawn);
    } else {
        cache_query(&mut fun, args.delegate, args.spawn);
    }

    quote!(#fun).into()
}

fn basic_query(fun: &mut ItemFn, delegate: bool, spawn: bool) {
    // TODO
    let _ = spawn;

    let is_async = fun.sig.asyncness.is_some();
    fun.sig.asyncness = None;
    replace_output(fun);

    let new = if delegate {
        quote!(delegate)
    } else {
        quote!(new)
    };

    let block = &fun.block;
    let block = if is_async {
        quote!(#new(async move #block))
    } else {
        quote!(from(#block))
    };

    *fun.block = syn::parse_quote!({
        ::duvet_core::Query::#block
    });
}

fn global_query(fun: &mut ItemFn, delegate: bool, spawn: bool) {
    // TODO
    let _ = spawn;

    let is_async = fun.sig.asyncness.is_some();
    fun.sig.asyncness = None;
    replace_output(fun);

    if !fun.sig.inputs.is_empty() {
        panic!("global query arguments must be empty");
    }

    let new = if delegate {
        quote!(delegate)
    } else {
        quote!(new)
    };
    let block = &fun.block;
    let block = if is_async {
        quote!(#new(async #block))
    } else {
        quote!(from(#block))
    };
    *fun.block = syn::parse_quote!({
        #[derive(Copy, Clone, Hash, PartialEq, Eq)]
        struct Query;
        ::duvet_core::Cache::current().get_or_init_global(Query, move || {
            ::duvet_core::Query::#block
        })
    });
}

fn cache_query(fun: &mut ItemFn, delegate: bool, spawn: bool) {
    // TODO
    let _ = spawn;

    let is_async = fun.sig.asyncness.is_some();
    fun.sig.asyncness = None;
    replace_output(fun);

    let mut inject_tokens = quote!();
    let mut join_alias = quote!();
    let mut join_args = quote!();
    let mut hash = quote!();

    for input in core::mem::take(&mut fun.sig.inputs).into_pairs() {
        let (mut input, punc) = input.into_tuple();

        let mut should_push = true;

        if let syn::FnArg::Typed(ref mut input) = input {
            let mut is_ignored = false;
            let mut inject = None;

            // TODO add custom hasher attribute

            input.attrs.retain(|attr| {
                if attr.path.is_ident("skip") {
                    is_ignored = true;
                    false
                } else if attr.path.is_ident("inject") {
                    inject = Some(attr.tokens.clone());
                    should_push = false;
                    false
                } else {
                    true
                }
            });

            if !is_ignored {
                let pat = &input.pat;

                if let Some(inject) = inject {
                    quote!(#[allow(unused_parens)] let #pat = #inject;)
                        .to_tokens(&mut inject_tokens);
                }

                if is_query_arg(&input.ty) {
                    quote!(let #pat = #pat.get();).to_tokens(&mut join_alias);
                    quote!(#pat,).to_tokens(&mut join_args);
                }

                quote!(::core::hash::Hash::hash(&#pat, &mut hasher);).to_tokens(&mut hash);
            }
        }

        if should_push {
            fun.sig.inputs.push(input);
            if let Some(punc) = punc {
                fun.sig.inputs.push_punct(punc);
            }
        }
    }

    let new = if delegate {
        quote!(delegate)
    } else {
        quote!(new)
    };
    let block = &fun.block;
    let block = if is_async {
        quote!(#new(async move #block))
    } else {
        quote!(from(#block))
    };
    *fun.block = syn::parse_quote!({
        ::duvet_core::Query::delegate(async move {
            #inject_tokens

            let key = {
                use ::duvet_core::macro_support::tokio;

                #join_alias
                let (#join_args) = tokio::join!(#join_args);

                let mut hasher = ::duvet_core::hash::Hasher::default();

                #hash

                hasher.finish()
            };

            ::duvet_core::Cache::current().get_or_init(key, move || {
                ::duvet_core::Query::#block
            })
        })
    });
}

fn is_query_arg(ty: &syn::Type) -> bool {
    if let syn::Type::Path(path) = ty {
        if path.qself.is_some() {
            return false;
        }

        if path.path.leading_colon.is_some() {
            return false;
        }

        if path.path.segments.len() != 1 {
            return false;
        }

        let seg = &path.path.segments[0];

        if seg.ident != "Query" {
            return false;
        }

        if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
            args.args.len() == 1
        } else {
            false
        }
    } else {
        false
    }
}

fn replace_output(fun: &mut ItemFn) -> Box<syn::Type> {
    let output = core::mem::replace(&mut fun.sig.output, syn::ReturnType::Default);
    match output {
        syn::ReturnType::Default => {
            todo!("cannot return an empty query");
        }
        syn::ReturnType::Type(arrow, ty) => {
            fun.sig.output =
                syn::ReturnType::Type(arrow, Box::new(syn::parse_quote!(::duvet_core::Query<#ty>)));
            ty
        }
    }
}
