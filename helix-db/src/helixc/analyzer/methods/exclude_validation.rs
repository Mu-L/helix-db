use crate::helixc::analyzer::error_codes::ErrorCode;
use crate::{
    generate_error,
    helixc::{
        analyzer::{
            analyzer::Ctx,
            errors::{push_query_err, push_query_err_with_fix},
            fix::Fix,
            types::Type,
        },
        parser::{types::*, location::Loc},
    },
};
use paste::paste;
use std::{borrow::Cow, collections::HashMap};

/// Iterates through the fields to exclude and validates that the exist on the type and have not been excluded previously.
///
/// # Arguments
///
/// * `ctx` - The context of the query
/// * `ex` - The exclude fields to validate
/// * `field_set` - The set of fields to validate
/// * `excluded` - The excluded fields
/// * `original_query` - The original query
/// * `type_name` - The name of the type
/// * `type_kind` - The kind of the type
/// * `span` - The span of the exclude fields
pub(crate) fn validate_exclude_fields<'a>(
    ctx: &mut Ctx<'a>,
    ex: &Exclude,
    field_set: &HashMap<&str, Cow<'a, Field>>,
    excluded: &HashMap<&str, Loc>,
    original_query: &'a Query,
    type_name: &str,
    type_kind: &str,
    span: Option<Loc>,
) {
    for (loc, key) in &ex.fields {
        if let Some(loc) = excluded.get(key.as_str()) {
            push_query_err_with_fix(
                ctx,
                original_query,
                loc.clone(),
                ErrorCode::E643,
                key.clone(),
                "remove the exclusion of `{}`",
                Fix::new(span.clone(), Some(loc.clone()), None),
            );
        } else if !field_set.contains_key(key.as_str()) {
            generate_error!(
                ctx,
                original_query,
                loc.clone(),
                E202,
                key.as_str(),
                type_kind,
                type_name
            );
        }
    }
}

/// Validates the exclude fields for a given type
///
/// # Arguments
///
/// * `ctx` - The context of the query
/// * `cur_ty` - The current type of the traversal
/// * `tr` - The traversal to validate
/// * `ex` - The exclude fields to validate
/// * `excluded` - The excluded fields
/// * `original_query` - The original query
pub(crate) fn validate_exclude<'a>(
    ctx: &mut Ctx<'a>,
    cur_ty: &Type,
    tr: &Traversal,
    ex: &Exclude,
    excluded: &HashMap<&str, Loc>,
    original_query: &'a Query,
) {
    match &cur_ty {
        Type::Nodes(Some(node_ty)) | Type::Node(Some(node_ty)) => {
            if let Some(field_set) = ctx.node_fields.get(node_ty.as_str()).cloned() {
                validate_exclude_fields(
                    ctx,
                    ex,
                    &field_set,
                    excluded,
                    original_query,
                    node_ty,
                    "node",
                    Some(tr.loc.clone()),
                );
            }
        }
        Type::Edges(Some(edge_ty)) | Type::Edge(Some(edge_ty)) => {
            if let Some(field_set) = ctx.edge_fields.get(edge_ty.as_str()).cloned() {
                validate_exclude_fields(
                    ctx,
                    ex,
                    &field_set,
                    excluded,
                    original_query,
                    edge_ty,
                    "edge",
                    Some(tr.loc.clone()),
                );
            }
        }
        Type::Vectors(Some(vector_ty)) | Type::Vector(Some(vector_ty)) => {
            if let Some(fields) = ctx.vector_fields.get(vector_ty.as_str()).cloned() {
                validate_exclude_fields(
                    ctx,
                    ex,
                    &fields,
                    excluded,
                    original_query,
                    vector_ty,
                    "vector",
                    Some(tr.loc.clone()),
                );
            }
        }
        Type::Anonymous(ty) => {
            // validates the exclude on the inner type of the anonymous type
            validate_exclude(ctx, ty, tr, ex, excluded, original_query);
        }
        _ => {
            generate_error!(ctx, original_query, ex.fields[0].0.clone(), E203, cur_ty.kind_str());
        }
    }
}
