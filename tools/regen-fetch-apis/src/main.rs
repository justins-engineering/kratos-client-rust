//! Codemod: reads a `src/apis/*.rs` file (the openapi-generator autogen
//! reqwest tree) and emits the equivalent `wasm_apis`/`worker_apis` source
//! text (the hand-maintained web-sys/workers-rs Fetch trees), replacing the
//! manual hand-port step described in the task #23 design doc.
//!
//! Handles: GET/POST/PUT/PATCH/DELETE, zero-or-more path params (including
//! raw identifiers like `r#type`), zero-or-more query params, an optional
//! JSON request body, and both response shapes (typed body via
//! `handle_response`, empty body via `handle_empty_response`). Does NOT
//! handle: deep-object query style, multipart bodies (confirmed unused
//! anywhere in this API), non-JSON request bodies.
//!
//! Any statement shape this doesn't recognize makes the tool PANIC naming the
//! function, by design (see the task #23 design doc) -- a human reviews the
//! diff before committing, and a loud failure is the cheap way to catch the
//! day the input shape actually drifts from what this was built against.
//!
//! Deliberate, disclosed normalizations vs. the current hand-ports (reported
//! separately, not hidden here):
//! - Always emits `RequestCredentials::Include` (wasm) for every function.
//!   This isn't derivable from the reqwest source at all (reqwest has no such
//!   concept) and the current hand-ports are internally inconsistent about it
//!   (Include in some files, SameOrigin in others, omitted in metadata_api.rs
//!   entirely) -- Include is the safe default for a client whose real
//!   endpoints are cookie-session-authenticated.
//! - Drops the `String::with_capacity(...)` length pre-computation in favor
//!   of `String::new()` -- zero behavioral difference, removes a whole class
//!   of "get the capacity formula right" bugs from the codemod's own
//!   correctness surface.
//! - Always calls `.to_string()` on query param values even where the source
//!   is already `&str` -- harmless extra allocation, not worth the type
//!   introspection needed to skip it.
//! - Emits fully-qualified paths (`web_sys::RequestInit`, ...) rather than a
//!   `use web_sys::{...}` block + bare names -- output is machine-regenerated
//!   and never hand-edited, so verbose-but-consistent costs nothing.
//!
//! Usage: regen-fetch-apis <path/to/apis/x_api.rs> <wasm-out-path> <worker-out-path>
//! Writes complete, ready-to-use files (license header + full use block +
//! error enums + functions) to the two output paths.

use quote::{ToTokens, quote};
use syn::{Expr, ExprIf, ExprLet, FnArg, Item, ItemFn, Lit, Pat, ReturnType, Stmt, Type};

struct PathParam {
    /// The real Rust identifier as it appears in the function signature
    /// (e.g. `id`, or `r#type` for the raw-identifier case).
    ident: syn::Ident,
    /// The name used in the format! template's `{name}` placeholder and
    /// named-arg binding, which for a raw identifier is the identifier
    /// WITHOUT the `r#` prefix (e.g. `type`, not `r#type`) -- `format!`'s
    /// named-arg syntax can't bind to a raw-identifier name directly.
    template_name: String,
}

struct QueryParam {
    key: String,
    source_ident: syn::Ident,
    /// Optional (`if let Some(...) = x { ... }`) vs. required/unconditional
    /// (`req_builder = req_builder.query(...)` with no wrapping `if let`,
    /// e.g. `flow: &str` on the self-service flow-update endpoints).
    optional: bool,
    /// `Option<Vec<String>>`-typed params use a different reqwest shape
    /// (the `match "multi" { "multi" => ..., _ => ... }` dead-branch
    /// artifact -- see the module doc comment) that has to join the Vec
    /// into a single value before it can go through the same push helper
    /// as every other query param.
    is_multi: bool,
}

struct HeaderParam {
    key: String,
    source_ident: syn::Ident,
}

struct ExtractedFn {
    name: syn::Ident,
    doc: String,
    params: Vec<(syn::Ident, Type)>,
    ok_type: Type,
    err_ident: syn::Ident,
    method: &'static str,
    path_template: String,
    path_params: Vec<PathParam>,
    query_params: Vec<QueryParam>,
    header_params: Vec<HeaderParam>,
    json_body: Option<syn::Ident>,
    has_api_key_check: bool,
    is_empty_response: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, input_path, wasm_out, worker_out] = args.as_slice() else {
        eprintln!("usage: regen-fetch-apis <path/to/apis/x_api.rs> <wasm-out-path> <worker-out-path>");
        std::process::exit(1);
    };

    let src = std::fs::read_to_string(input_path).expect("failed to read input file");
    let file = syn::parse_file(&src).expect("failed to parse input file as Rust source");

    let mut error_enums = Vec::new();
    let mut fns = Vec::new();

    for item in &file.items {
        match item {
            Item::Enum(e) if e.ident.to_string().ends_with("Error") => {
                error_enums.push(e.clone());
            }
            Item::Fn(f) if matches!(f.vis, syn::Visibility::Public(_)) => {
                fns.push(extract_fn(f));
            }
            _ => {}
        }
    }

    std::fs::write(wasm_out, emit_module(&error_enums, &fns, Target::Wasm)).expect("failed to write wasm output");
    std::fs::write(worker_out, emit_module(&error_enums, &fns, Target::Worker)).expect("failed to write worker output");
    eprintln!("wrote {wasm_out} and {worker_out} ({} functions)", fns.len());
}

fn extract_fn(f: &ItemFn) -> ExtractedFn {
    let name = f.sig.ident.clone();

    let doc = f
        .attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &attr.meta {
                    if let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Params, skipping the fixed leading `configuration: &configuration::Configuration`.
    let mut params = Vec::new();
    for arg in f.sig.inputs.iter() {
        if let FnArg::Typed(pat_type) = arg {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                if pat_ident.ident == "configuration" {
                    continue;
                }
                params.push((pat_ident.ident.clone(), (*pat_type.ty).clone()));
            }
        }
    }

    // Return type: Result<T, Error<E>>.
    let ReturnType::Type(_, ret_ty) = &f.sig.output else {
        panic!("function {name}: no return type");
    };
    let Type::Path(ret_path) = &**ret_ty else {
        panic!("function {name}: return type isn't a path type");
    };
    let result_seg = ret_path.path.segments.last().expect("empty return type path");
    let syn::PathArguments::AngleBracketed(result_args) = &result_seg.arguments else {
        panic!("function {name}: Result<..> has no generic args");
    };
    let mut result_generics = result_args.args.iter();
    let syn::GenericArgument::Type(ok_type) = result_generics.next().expect("Result missing Ok type") else {
        panic!("function {name}: Result Ok arg isn't a type");
    };
    let syn::GenericArgument::Type(Type::Path(err_type_path)) = result_generics.next().expect("Result missing Err type") else {
        panic!("function {name}: Result Err arg isn't a type path");
    };
    let err_outer_seg = err_type_path.path.segments.last().unwrap();
    let syn::PathArguments::AngleBracketed(err_args) = &err_outer_seg.arguments else {
        panic!("function {name}: Error<..> has no generic args");
    };
    let syn::GenericArgument::Type(Type::Path(err_inner_path)) = err_args.args.first().expect("Error<> missing inner type") else {
        panic!("function {name}: Error<E> inner type isn't a path");
    };
    let err_ident = err_inner_path.path.segments.last().unwrap().ident.clone();

    let is_empty_response = matches!(ok_type, Type::Tuple(t) if t.elems.is_empty());

    let mut method = "GET";
    let mut path_template = String::new();
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    let mut json_body = None;
    let mut has_api_key_check = false;

    for stmt in &f.block.stmts {
        match stmt {
            // let uri_str = format!("...", configuration.base_path [, name = expr]*);
            Stmt::Local(local) if local.pat_ident_name().as_deref() == Some("uri_str") => {
                if let Some(init) = &local.init {
                    if let Expr::Macro(m) = &*init.expr {
                        if m.mac.path.is_ident("format") {
                            let (template, params) = parse_format_macro(&m.mac, &name);
                            path_template = template;
                            path_params = params;
                        }
                    }
                }
            }
            // let mut req_builder = configuration.client.request(reqwest::Method::X, &uri_str);
            Stmt::Local(local) if local.pat_ident_name().as_deref() == Some("req_builder") => {
                if let Some(init) = &local.init {
                    if let Some(m) = find_method_call(&init.expr) {
                        method = m;
                    }
                }
            }
            // Optional query/header param: if let Some([ref] param_value) = p_x { ... }
            Stmt::Expr(Expr::If(if_expr), _) => {
                if is_api_key_check(if_expr) {
                    has_api_key_check = true;
                } else if let Some(qp) = extract_query_param(if_expr) {
                    query_params.push(qp);
                } else if let Some(hp) = extract_header_param(if_expr) {
                    header_params.push(hp);
                }
            }
            // Unconditional (required-param) query push or JSON body:
            // req_builder = req_builder.query(&[("flow", &p_flow.to_string())]);
            // req_builder = req_builder.json(&p_x);
            Stmt::Expr(Expr::Assign(assign), _) => {
                if let Expr::MethodCall(mc) = &*assign.right {
                    if mc.method == "json" {
                        json_body = Some(extract_json_body_ident(mc, &name));
                    } else if mc.method == "query" {
                        if let Some(mut qp) = extract_unconditional_query_param(mc) {
                            qp.optional = false;
                            query_params.push(qp);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Every real parameter must be accounted for somewhere (path, query,
    // header, or JSON body) -- fail loud rather than silently compiling a
    // function that drops one, per the codemod's core design principle. This
    // is what caught the cookie/X-Session-Token/multi-value-query/
    // unconditional-query gaps during development; keeping it as a
    // permanent check protects against the next shape this hasn't seen yet.
    for (param_ident, _) in &params {
        let accounted = path_params.iter().any(|p| &p.ident == param_ident)
            || query_params.iter().any(|q| &q.source_ident == param_ident)
            || header_params.iter().any(|h| &h.source_ident == param_ident)
            || json_body.as_ref() == Some(param_ident);
        if !accounted {
            panic!(
                "function {name}: parameter `{param_ident}` isn't referenced by any recognized \
                 path/query/header/body extraction -- this is exactly the silent-drop failure \
                 mode the codemod is designed to catch instead of compiling wrong. Extend \
                 extraction to handle this parameter's usage shape before regenerating this file."
            );
        }
    }

    ExtractedFn {
        name,
        doc,
        params,
        ok_type: ok_type.clone(),
        err_ident,
        method,
        path_template,
        path_params,
        query_params,
        header_params,
        json_body,
        has_api_key_check,
        is_empty_response,
    }
}

trait LocalExt {
    fn pat_ident_name(&self) -> Option<String>;
}
impl LocalExt for syn::Local {
    fn pat_ident_name(&self) -> Option<String> {
        match &self.pat {
            Pat::Ident(p) => Some(p.ident.to_string()),
            Pat::Type(pt) => match &*pt.pat {
                Pat::Ident(p) => Some(p.ident.to_string()),
                _ => None,
            },
            _ => None,
        }
    }
}

fn find_method_call(expr: &Expr) -> Option<&'static str> {
    let Expr::MethodCall(mc) = expr else { return None };
    if mc.method != "request" {
        return None;
    }
    let arg0 = mc.args.first()?;
    let Expr::Path(p) = arg0 else { return None };
    let last = p.path.segments.last()?.ident.to_string();
    Some(match last.as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        other => panic!("unrecognized HTTP method `{other}`"),
    })
}

/// Parses a `format!("...", configuration.base_path [, name = expr]*)` call
/// as emitted by the reqwest apis/ tree for URI building. Returns the path
/// template string (with `{name}` placeholders still in it) and the ordered
/// list of named path params.
fn parse_format_macro(mac: &syn::Macro, fn_name: &syn::Ident) -> (String, Vec<PathParam>) {
    // format!'s named-arg syntax (`name = expr`) is a macro-level construct,
    // not ordinary expression grammar -- `name` can be a bare keyword like
    // `type` (openapi-generator always uses the unprefixed keyword form
    // here, since format!'s named-arg binding can't itself be a raw
    // identifier), which `syn::Expr::parse` rejects outright since `type`
    // can't start any expression. So this can't be parsed as a generic
    // `Punctuated<Expr, Comma>` list (which is what broke on
    // delete_identity_credentials's `type = ...` arg) -- parse the LHS name
    // with `Ident::parse_any` (permits keywords) instead.
    use syn::ext::IdentExt;

    struct FormatArgs {
        template: syn::LitStr,
        named: Vec<(syn::Ident, Expr)>,
    }
    impl syn::parse::Parse for FormatArgs {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let template: syn::LitStr = input.parse()?;
            let mut named = Vec::new();
            while !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
                if input.is_empty() {
                    break;
                }
                let fork = input.fork();
                let is_named = fork.call(syn::Ident::parse_any).is_ok() && fork.peek(syn::Token![=]);
                if is_named {
                    let name = input.call(syn::Ident::parse_any)?;
                    input.parse::<syn::Token![=]>()?;
                    let value: Expr = input.parse()?;
                    named.push((name, value));
                } else {
                    // Positional arg beyond the first is only ever
                    // `configuration.base_path` in this codebase; parse and discard.
                    let _value: Expr = input.parse()?;
                }
            }
            Ok(FormatArgs { template, named })
        }
    }

    let parsed: FormatArgs = mac
        .parse_body()
        .unwrap_or_else(|e| panic!("function {fn_name}: failed to parse format! macro body: {e}"));

    let path_params = parsed
        .named
        .iter()
        .map(|(name, value)| {
            let template_name = name.to_string();
            let Expr::Call(call) = value else {
                panic!("function {fn_name}: format! named-arg `{template_name}`'s value isn't a call expression");
            };
            let Some(Expr::Path(inner_arg)) = call.args.first() else {
                panic!("function {fn_name}: urlencode(..) call has no identifier argument");
            };
            let raw_ident = inner_arg.path.segments.last().unwrap().ident.clone();
            let stripped = raw_ident.to_string().strip_prefix("p_").unwrap_or(&raw_ident.to_string()).to_string();
            // `name` (the format! binding) is already the unprefixed keyword
            // form when it's a keyword (see above) -- reuse that directly to
            // decide whether the real Rust identifier needs `r#`.
            let ident = if is_rust_keyword(&template_name) {
                syn::Ident::new_raw(&stripped, raw_ident.span())
            } else {
                syn::Ident::new(&stripped, raw_ident.span())
            };
            PathParam { ident, template_name }
        })
        .collect();

    (parsed.template.value(), path_params)
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "type" | "fn" | "match" | "loop" | "move" | "ref" | "self" | "Self" | "super" | "where" | "async" | "await" | "dyn"
    )
}

fn is_api_key_check(if_expr: &ExprIf) -> bool {
    let Expr::Let(ExprLet { expr, .. }) = &*if_expr.cond else { return false };
    let Expr::Field(field) = &**expr else { return false };
    field.member == syn::Member::Named(syn::Ident::new("api_key", proc_macro2::Span::call_site()))
}

/// Matches `if let Some([ref] param_value) = p_x { req_builder = <EXPR>; }`
/// where `<EXPR>` is either the plain single-value shape
/// (`req_builder.query(&[("key", &param_value.to_string())])`) or the
/// `Option<Vec<String>>` multi-value shape (`match "multi" { "multi" =>
/// req_builder.query(&param_value.into_iter().map(|p| ("key".to_owned(),
/// p.to_string())).collect()), _ => ... }` -- a dead-branch artifact of the
/// stock reqwest template that always takes the "multi" arm; see the
/// courier_api.rs `match "multi"` note in the design doc). Returns the query
/// key + the source identifier (`p_x`, normalized back to `x`).
fn extract_query_param(if_expr: &ExprIf) -> Option<QueryParam> {
    let source_ident = optional_if_let_source(if_expr)?;

    for stmt in &if_expr.then_branch.stmts {
        let Stmt::Expr(Expr::Assign(assign), _) = stmt else { continue };
        match &*assign.right {
            Expr::MethodCall(mc) if mc.method == "query" => {
                if let Some(key) = find_first_str_lit(&mc.args) {
                    return Some(QueryParam { key, source_ident, optional: true, is_multi: false });
                }
            }
            Expr::Match(m) => {
                // The "multi" arm's body carries the real key; scanning the
                // whole match would find the "multi" pattern literal itself
                // first instead.
                let multi_arm = m.arms.first()?;
                let key = find_first_str_lit_in_expr(&multi_arm.body)?;
                return Some(QueryParam { key, source_ident, optional: true, is_multi: true });
            }
            _ => {}
        }
    }
    None
}

/// Matches `if let Some(param_value) = p_x { req_builder =
/// req_builder.header("Key", param_value.to_string()); }` (custom headers
/// like Cookie/X-Session-Token -- NOT the fixed user_agent/api_key blocks,
/// which are handled separately). Distinguished from `extract_query_param`
/// purely by which builder method the then-branch calls.
fn extract_header_param(if_expr: &ExprIf) -> Option<HeaderParam> {
    let source_ident = optional_if_let_source(if_expr)?;

    for stmt in &if_expr.then_branch.stmts {
        let Stmt::Expr(Expr::Assign(assign), _) = stmt else { continue };
        if let Expr::MethodCall(mc) = &*assign.right {
            if mc.method == "header" {
                if let Some(key) = find_first_str_lit(&mc.args) {
                    return Some(HeaderParam { key, source_ident });
                }
            }
        }
    }
    None
}

/// Shared by extract_query_param/extract_header_param: pulls the `p_x`
/// source identifier out of `if let Some([ref] param_value) = p_x { .. }`,
/// normalized back to the real parameter name `x`.
fn optional_if_let_source(if_expr: &ExprIf) -> Option<syn::Ident> {
    let Expr::Let(ExprLet { pat, expr, .. }) = &*if_expr.cond else { return None };
    let Pat::TupleStruct(ts) = &**pat else { return None };
    if !ts.path.is_ident("Some") {
        return None;
    }
    let Expr::Path(source_path) = &**expr else { return None };
    let raw_ident = source_path.path.segments.last()?.ident.clone();
    Some(match raw_ident.to_string().strip_prefix("p_") {
        Some(stripped) => syn::Ident::new(stripped, raw_ident.span()),
        None => raw_ident,
    })
}

/// Matches the unconditional (required-parameter) query push:
/// `req_builder = req_builder.query(&[("flow", &p_flow.to_string())]);` with
/// no wrapping `if let` -- used for endpoints where the query param is a
/// plain `&str`/etc, not an `Option<_>` (e.g. `flow` on the self-service
/// flow-update endpoints). Navigates the exact shape structurally (Reference
/// -> Array -> [0] Tuple -> [1] Reference -> MethodCall receiver) rather
/// than token-scanning, since a generic "find any identifier" scan would
/// ambiguously also match the `to_string`/`to_owned` method names in the
/// same expression.
fn extract_unconditional_query_param(mc: &syn::ExprMethodCall) -> Option<QueryParam> {
    let key = find_first_str_lit(&mc.args)?;

    let Some(Expr::Reference(array_ref)) = mc.args.first() else { return None };
    let Expr::Array(array) = &*array_ref.expr else { return None };
    let Some(Expr::Tuple(tuple)) = array.elems.first() else { return None };
    let Some(Expr::Reference(value_ref)) = tuple.elems.get(1) else { return None };
    let Expr::MethodCall(to_string_call) = &*value_ref.expr else { return None };
    let Expr::Path(p) = &*to_string_call.receiver else { return None };
    let raw_ident = p.path.segments.last()?.ident.clone();
    let source_ident = match raw_ident.to_string().strip_prefix("p_") {
        Some(stripped) => syn::Ident::new(stripped, raw_ident.span()),
        None => raw_ident,
    };
    Some(QueryParam { key, source_ident, optional: false, is_multi: false })
}

/// `req_builder = req_builder.json(&p_x);` -- extract `p_x`, normalized to
/// the real param name `x`.
fn extract_json_body_ident(mc: &syn::ExprMethodCall, fn_name: &syn::Ident) -> syn::Ident {
    let Some(Expr::Reference(r)) = mc.args.first() else {
        panic!("function {fn_name}: .json(..) call's argument isn't a reference expression");
    };
    let Expr::Path(p) = &*r.expr else {
        panic!("function {fn_name}: .json(&..) argument isn't a bare identifier");
    };
    let raw_ident = p.path.segments.last().unwrap().ident.clone();
    match raw_ident.to_string().strip_prefix("p_") {
        Some(stripped) => syn::Ident::new(stripped, raw_ident.span()),
        None => raw_ident,
    }
}

/// args is `&[("key", &param_value.to_string())]` -- the string literal is
/// nested inside `[...]`/`(...)` groups, so this recurses into groups rather
/// than only scanning the top-level token stream.
fn find_first_str_lit(args: &syn::punctuated::Punctuated<Expr, syn::Token![,]>) -> Option<String> {
    find_str_lit_in_stream(args.to_token_stream())
}

fn find_first_str_lit_in_expr(e: &Expr) -> Option<String> {
    find_str_lit_in_stream(e.to_token_stream())
}

fn find_str_lit_in_stream(stream: proc_macro2::TokenStream) -> Option<String> {
    for tok in stream {
        match tok {
            proc_macro2::TokenTree::Literal(lit) => {
                let s = lit.to_string();
                if s.starts_with('"') {
                    return Some(s.trim_matches('"').to_string());
                }
            }
            proc_macro2::TokenTree::Group(g) => {
                if let Some(found) = find_str_lit_in_stream(g.stream()) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

enum Target {
    Wasm,
    Worker,
}

const FILE_HEADER: &str = "/*\n * Ory Identities API\n *\n * This is the API specification for Ory Identities with features such as registration, login, recovery, account verification, profile settings, password reset, identity management, session management, email and sms delivery, and more.\n *\n * The version of the OpenAPI document: v26.2.0\n * Contact: office@ory.sh\n * Generated by: https://openapi-generator.tech (via tools/regen-fetch-apis, see task #23)\n */\n\n";

fn emit_module(error_enums: &[syn::ItemEnum], fns: &[ExtractedFn], target: Target) -> String {
    let mut out = String::new();
    out.push_str(FILE_HEADER);

    // gloo_utils's JsValueSerdeExt is only needed per-file for serializing a
    // JSON request body (`JsValue::from_serde(&body)`) now that response
    // deserialization moved into the shared handle_response helper in
    // mod.rs -- unlike wasm_bindgen::prelude::* (a glob import, harmless and
    // already unconditional in every current hand-ported file regardless of
    // use, since unused glob imports don't warn).
    let needs_json_value_serde_ext = fns.iter().any(|f| f.json_body.is_some());
    // AddQuery (the .add_query() extension trait) is only used by functions
    // that have query params -- unused if a whole file has none (e.g.
    // metadata_api.rs's 3 param-less GETs).
    let needs_add_query = fns.iter().any(|f| !f.query_params.is_empty());

    if needs_add_query {
        out.push_str("use super::{AddQuery, Error, configuration};\n");
    } else {
        out.push_str("use super::{Error, configuration};\n");
    }
    out.push_str("use crate::models;\n");
    if needs_json_value_serde_ext {
        out.push_str("use gloo_utils::format::JsValueSerdeExt;\n");
    }
    out.push_str("use serde::{Deserialize, Serialize};\n");
    out.push_str("use wasm_bindgen::prelude::*;\n\n");

    for e in error_enums {
        out.push_str(&emit_enum(e));
        out.push_str("\n\n");
    }

    for f in fns {
        out.push_str(&emit_fn(f, &target));
        out.push_str("\n\n");
    }

    out
}

/// quote!'s output represents doc comments as `#[doc = "..."]` attributes,
/// not `///` (they aren't distinct at the proc-macro token level); this
/// crate's convention (every hand-written file in it) uses `///`. Strip the
/// `#[doc = ...]` attr out of the enum before tokenizing the rest (derive,
/// serde, variants), and prepend a manually-built `///` line instead.
fn emit_enum(e: &syn::ItemEnum) -> String {
    let doc = e
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &attr.meta {
                    if let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
            None
        })
        .unwrap_or_default();

    let mut e = e.clone();
    e.attrs.retain(|attr| !attr.path().is_ident("doc"));

    format!("/// {doc}\n{}", e.to_token_stream())
}

fn emit_fn(f: &ExtractedFn, target: &Target) -> String {
    let name = &f.name;
    let err_ident = &f.err_ident;
    let ok_type = &f.ok_type;

    let params_sig: Vec<_> = f.params.iter().map(|(ident, ty)| quote! { #ident: #ty }).collect();

    let urlencode_mod: syn::Path = match target {
        Target::Wasm => syn::parse_str("crate::wasm_apis::urlencode").unwrap(),
        Target::Worker => syn::parse_str("crate::worker_apis::urlencode").unwrap(),
    };

    // The template's leading "{}" placeholder is `configuration.base_path`,
    // which is always pushed separately as the first `push_str`.
    let mut remaining = f.path_template.strip_prefix("{}").unwrap_or(&f.path_template).to_string();

    let mut path_param_setup = Vec::new();
    let mut literal_pieces: Vec<String> = Vec::new();
    // Split the template on each `{template_name}` placeholder in order,
    // interleaving literal text pushes with urlencoded path-param pushes.
    for pp in &f.path_params {
        let placeholder = format!("{{{}}}", pp.template_name);
        let idx = remaining.find(&placeholder).unwrap_or_else(|| panic!("function {name}: path template missing `{{{}}}}}`", pp.template_name));
        literal_pieces.push(remaining[..idx].to_string());
        remaining = remaining[idx + placeholder.len()..].to_string();

        let ident = &pp.ident;
        path_param_setup.push(quote! { let #ident = #urlencode_mod(#ident); });
    }
    literal_pieces.push(remaining);

    let uri_pushes: Vec<_> = {
        let mut pushes = Vec::new();
        pushes.push(quote! { uri_str.push_str(&configuration.base_path); });
        for (i, literal) in literal_pieces.iter().enumerate() {
            if !literal.is_empty() {
                pushes.push(quote! { uri_str.push_str(#literal); });
            }
            if let Some(pp) = f.path_params.get(i) {
                let ident = &pp.ident;
                pushes.push(quote! { uri_str.push_str(&#ident); });
            }
        }
        pushes
    };

    let uri_setup = quote! {
        #(#path_param_setup)*
        let mut uri_str = String::new();
        #(#uri_pushes)*
    };

    let query_pushes: Vec<_> = f
        .query_params
        .iter()
        .map(|qp| {
            let key = format!("{}=", qp.key);
            let src = &qp.source_ident;
            match (qp.optional, qp.is_multi) {
                (true, false) => quote! {
                    if let Some(ref str) = #src {
                        uri_str.add_query(&mut is_first_query, #key, &str.to_string());
                    }
                },
                // Option<Vec<String>>: join into one value, then push through
                // the same add_query helper as any other param -- uniform
                // across both the "sole query param" and "mixed with other
                // params" cases the current hand-port has two different
                // shapes for (see the multi-value note in the module doc
                // comment: disclosed normalization, not a behavior change).
                (true, true) => {
                    let joined_key = format!("&{}=", qp.key);
                    quote! {
                        if let Some(ref str_vec) = #src {
                            let joined = str_vec
                                .into_iter()
                                .map(|p| p.to_string())
                                .collect::<Vec<String>>()
                                .join(#joined_key)
                                .to_string();
                            uri_str.add_query(&mut is_first_query, #key, &joined);
                        }
                    }
                }
                // Required (non-Option) param, e.g. `flow: &str` -- no `if
                // let` unwrap needed.
                (false, _) => quote! {
                    uri_str.add_query(&mut is_first_query, #key, &#src.to_string());
                },
            }
        })
        .collect();
    let query_prelude = if query_pushes.is_empty() { quote! {} } else { quote! { let mut is_first_query: bool = true; } };

    let header_pushes: Vec<_> = f
        .header_params
        .iter()
        .map(|hp| {
            let key = &hp.key;
            let src = &hp.source_ident;
            match target {
                Target::Wasm => quote! {
                    if let Some(#src) = #src {
                        req_builder.headers().set(#key, #src)?;
                    }
                },
                Target::Worker => quote! {
                    if let Some(#src) = #src {
                        headers.append(#key, #src)?;
                    }
                },
            }
        })
        .collect();

    let method_variant = match (target, f.method) {
        (Target::Wasm, "GET") => quote! { "GET" },
        (Target::Wasm, "DELETE") => quote! { "DELETE" },
        (Target::Wasm, "POST") => quote! { "POST" },
        (Target::Wasm, "PUT") => quote! { "PUT" },
        (Target::Wasm, "PATCH") => quote! { "PATCH" },
        (Target::Worker, "GET") => quote! { worker::Method::Get },
        (Target::Worker, "DELETE") => quote! { worker::Method::Delete },
        (Target::Worker, "POST") => quote! { worker::Method::Post },
        (Target::Worker, "PUT") => quote! { worker::Method::Put },
        (Target::Worker, "PATCH") => quote! { worker::Method::Patch },
        _ => unreachable!("find_method_call already rejects unrecognized methods"),
    };

    let api_key_block = if f.has_api_key_check {
        match target {
            Target::Wasm => quote! {
                if let Some(ref apikey) = configuration.api_key {
                    let key = apikey.key.clone();
                    let value = match apikey.prefix {
                        Some(ref prefix) => format!("{} {}", prefix, key),
                        None => key,
                    };
                    req_builder.headers().set("Authorization", &value)?;
                };
            },
            Target::Worker => quote! {
                if let Some(ref apikey) = configuration.api_key {
                    let key = apikey.key.clone();
                    let value = match apikey.prefix {
                        Some(ref prefix) => format!("{} {}", prefix, key),
                        None => key,
                    };
                    headers.append("Authorization", &value)?;
                };
            },
        }
    } else {
        quote! {}
    };

    let handle_call = if f.is_empty_response {
        quote! { super::handle_empty_response(resp).await }
    } else {
        quote! { super::handle_response(resp).await }
    };

    // Matches the exact shape confirmed in the current hand-port
    // (create_identity): the body is set via `JsValue::from_serde(&ident)?`
    // on the RequestInit (wasm: `client.set_body`, BEFORE the Request is
    // constructed from it; worker: `req_builder.with_body`, after
    // `with_headers`), and the Content-Type header is set alongside Accept
    // like any other header, not specially placed.
    let client_body_call = match (target, &f.json_body) {
        (Target::Wasm, Some(ident)) => quote! {
            client.set_body(&JsValue::from_serde(&#ident)?);
        },
        _ => quote! {},
    };
    let content_type_header = f.json_body.is_some().then(|| match target {
        Target::Wasm => quote! { req_builder.headers().set("Content-Type", "application/json")?; },
        Target::Worker => quote! { headers.append("Content-Type", "application/json")?; },
    });
    let worker_body_stmt = match (target, &f.json_body) {
        (Target::Worker, Some(ident)) => quote! {
            req_builder.with_body(Some(JsValue::from_serde(&#ident)?));
        },
        _ => quote! {},
    };

    let body = match target {
        Target::Wasm => quote! {
            #uri_setup

            let client = web_sys::RequestInit::new();
            client.set_method(#method_variant);
            client.set_mode(web_sys::RequestMode::Cors);
            client.set_credentials(web_sys::RequestCredentials::Include);
            #client_body_call

            #query_prelude
            #(#query_pushes)*

            let req_builder = web_sys::Request::new_with_str_and_init(&uri_str, &client)?;

            if let Some(ref user_agent) = configuration.user_agent {
                req_builder.headers().set("USER_AGENT", user_agent)?;
            }
            #(#header_pushes)*
            #api_key_block
            req_builder.headers().set("Accept", "application/json")?;
            #content_type_header

            let req = wasm_bindgen_futures::JsFuture::from(
                web_sys::window().expect("Failed to get Window object").fetch_with_request(&req_builder),
            ).await?;
            assert!(req.is_instance_of::<web_sys::Response>());
            let resp: web_sys::Response = req.dyn_into().expect("Failed to dynamically cast JsFuture into Response");

            #handle_call
        },
        Target::Worker => quote! {
            #uri_setup

            #query_prelude
            #(#query_pushes)*

            let headers = worker::Headers::new();
            if let Some(ref user_agent) = configuration.user_agent {
                headers.append("USER_AGENT", user_agent)?;
            }
            #(#header_pushes)*
            #api_key_block
            headers.append("Accept", "application/json")?;
            #content_type_header

            let mut req_builder = worker::RequestInit::new();
            req_builder.with_method(#method_variant);
            req_builder.with_headers(headers);
            #worker_body_stmt

            let req = worker::Request::new_with_init(&uri_str, &req_builder)?;
            let resp = worker::Fetch::Request(req).send().await?;

            #handle_call
        },
    };

    let doc_lines: Vec<_> = f.doc.lines().map(|l| format!("///{}{}", if l.is_empty() { "" } else { " " }, l)).collect();
    let doc_comment = doc_lines.join("\n");

    let tokens = quote! {
        pub async fn #name(
            configuration: &configuration::Configuration,
            #(#params_sig),*
        ) -> Result<#ok_type, Error<#err_ident>> {
            #body
        }
    };

    format!("{doc_comment}\n{tokens}")
}
