//! Semantic analyzer for Helix‑QL.

use crate::generate_error;
use crate::helixc::analyzer::error_codes::ErrorCode;
use crate::helixc::{
    analyzer::{
        Ctx,
        errors::{push_query_err, push_query_warn},
        methods::{infer_expr_type::infer_expr_type, statement_validation::validate_statements},
        types::Type,
        utils::{is_valid_identifier, VariableInfo},
    },
    generator::{
        queries::{Parameter as GeneratedParameter, Query as GeneratedQuery},
        return_values::ReturnValue,
        source_steps::SourceStep,
        statements::Statement as GeneratedStatement,
        traversal_steps::ShouldCollect,
    },
    parser::{location::Loc, types::*},
};
use paste::paste;
use std::collections::HashMap;

/// Helper function to get Rust type string from analyzer Type and populate return value fields
fn type_to_rust_string_and_fields(
    ty: &Type,
    should_collect: &ShouldCollect,
    ctx: &Ctx,
    field_name: &str,
) -> (String, Vec<crate::helixc::generator::return_values::ReturnValueField>) {
    match (ty, should_collect) {
        // For single nodes/vectors/edges, generate a proper struct based on schema
        (Type::Node(Some(label)), ShouldCollect::ToObj | ShouldCollect::No) => {
            let type_name = format!("{}ReturnType", label);
            let mut fields = vec![
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                },
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "label".to_string(),
                    field_type: "String".to_string(),
                },
            ];

            // Add properties from schema (skip id and label as they're already added)
            if let Some(node_fields) = ctx.node_fields.get(label.as_str()) {
                for (prop_name, field) in node_fields {
                    if *prop_name != "id" && *prop_name != "label" {
                        fields.push(crate::helixc::generator::return_values::ReturnValueField {
                            name: prop_name.to_string(),
                            field_type: format!("{}", field.field_type),
                        });
                    }
                }
            }
            (type_name, fields)
        }
        (Type::Edge(Some(label)), ShouldCollect::ToObj | ShouldCollect::No) => {
            let type_name = format!("{}ReturnType", label);
            let mut fields = vec![
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                },
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "label".to_string(),
                    field_type: "String".to_string(),
                },
            ];

            if let Some(edge_fields) = ctx.edge_fields.get(label.as_str()) {
                for (prop_name, field) in edge_fields {
                    if *prop_name != "id" && *prop_name != "label" {
                        fields.push(crate::helixc::generator::return_values::ReturnValueField {
                            name: prop_name.to_string(),
                            field_type: format!("{}", field.field_type),
                        });
                    }
                }
            }
            (type_name, fields)
        }
        (Type::Vector(Some(label)), ShouldCollect::ToObj | ShouldCollect::No) => {
            let type_name = format!("{}ReturnType", label);
            let mut fields = vec![
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "id".to_string(),
                    field_type: "ID".to_string(),
                },
                crate::helixc::generator::return_values::ReturnValueField {
                    name: "label".to_string(),
                    field_type: "String".to_string(),
                },
            ];

            if let Some(vector_fields) = ctx.vector_fields.get(label.as_str()) {
                for (prop_name, field) in vector_fields {
                    if *prop_name != "id" && *prop_name != "label" {
                        fields.push(crate::helixc::generator::return_values::ReturnValueField {
                            name: prop_name.to_string(),
                            field_type: format!("{}", field.field_type),
                        });
                    }
                }
            }
            (type_name, fields)
        }
        // For Vec types, we still need Vec<TypeName>
        (Type::Node(Some(label)), ShouldCollect::ToVec) => (format!("Vec<{}ReturnType>", label), vec![]),
        (Type::Edge(Some(label)), ShouldCollect::ToVec) => (format!("Vec<{}ReturnType>", label), vec![]),
        (Type::Vector(Some(label)), ShouldCollect::ToVec) => (format!("Vec<{}ReturnType>", label), vec![]),
        // Fallbacks for None labels
        (Type::Node(None), _) | (Type::Edge(None), _) | (Type::Vector(None), _) => ("DynamicValue".to_string(), vec![]),
        (Type::Scalar(s), _) => (format!("{}", s), vec![]),
        (Type::Boolean, _) => ("bool".to_string(), vec![]),
        (Type::Array(inner), _) => {
            let (inner_type, _) = type_to_rust_string_and_fields(inner, &ShouldCollect::No, ctx, field_name);
            (format!("Vec<{}>", inner_type), vec![])
        }
        (Type::Aggregate, _) => ("DynamicValue".to_string(), vec![]),
        _ => ("DynamicValue".to_string(), vec![]),
    }
}

pub(crate) fn validate_query<'a>(ctx: &mut Ctx<'a>, original_query: &'a Query) {
    let mut query = GeneratedQuery {
        name: original_query.name.clone(),
        ..Default::default()
    };

    if let Some(BuiltInMacro::Model(model_name)) = &original_query.built_in_macro {
        // handle model macro
        query.embedding_model_to_use = Some(model_name.clone());
    }

    // -------------------------------------------------
    // Parameter validation
    // -------------------------------------------------
    for param in &original_query.parameters {
        if let FieldType::Identifier(ref id) = param.param_type.1
            && is_valid_identifier(ctx, original_query, param.param_type.0.clone(), id.as_str())
            && !ctx.node_set.contains(id.as_str())
            && !ctx.edge_map.contains_key(id.as_str())
            && !ctx.vector_set.contains(id.as_str())
        {
            generate_error!(
                ctx,
                original_query,
                param.param_type.0.clone(),
                E209,
                &id,
                &param.name.1
            );
        }
        // constructs parameters and sub‑parameters for generator
        GeneratedParameter::unwrap_param(
            param.clone(),
            &mut query.parameters,
            &mut query.sub_parameters,
        );
    }

    // -------------------------------------------------
    // Statement‑by‑statement walk
    // -------------------------------------------------
    let mut scope: HashMap<&str, VariableInfo> = HashMap::new();
    for param in &original_query.parameters {
        scope.insert(
            param.name.1.as_str(),
            VariableInfo::new(Type::from(param.param_type.1.clone()), false),
        );
    }
    for stmt in &original_query.statements {
        let statement = validate_statements(ctx, &mut scope, original_query, &mut query, stmt);
        if let Some(s) = statement {
            query.statements.push(s);
        } else {
            // given all erroneous statements are caught by the analyzer, this should never happen
            return;
        }
    }

    // -------------------------------------------------
    // Validate RETURN expressions
    // -------------------------------------------------
    if original_query.return_values.is_empty() {
        let end = original_query.loc.end;
        push_query_warn(
            ctx,
            original_query,
            Loc::new(
                original_query.loc.filepath.clone(),
                end,
                end,
                original_query.loc.span.clone(),
            ),
            ErrorCode::W101,
            "query has no RETURN clause".to_string(),
            "add `RETURN <expr>` at the end",
            None,
        );
    }
    for ret in &original_query.return_values {
        analyze_return_expr(ctx, original_query, &mut scope, &mut query, ret);
    }

    if let Some(BuiltInMacro::MCP) = &original_query.built_in_macro {
        if query.return_values.len() != 1 {
            generate_error!(
                ctx,
                original_query,
                original_query.loc.clone(),
                E401,
                &query.return_values.len().to_string()
            );
        }
        let return_name = query.return_values.first().unwrap().0.clone();
        query.mcp_handler = Some(return_name);
    }

    ctx.output.queries.push(query);
}

fn analyze_return_expr<'a>(
    ctx: &mut Ctx<'a>,
    original_query: &'a Query,
    scope: &mut HashMap<&'a str, VariableInfo>,
    query: &mut GeneratedQuery,
    ret: &'a ReturnType,
) {
    match ret {
        ReturnType::Expression(expr) => {
            let (inferred_type, stmt) = infer_expr_type(ctx, expr, scope, original_query, None, query);

            if stmt.is_none() {
                return;
            }

            match stmt.unwrap() {
                GeneratedStatement::Traversal(traversal) => {
                    match &traversal.source_step.inner() {
                        SourceStep::Identifier(v) => {
                            is_valid_identifier(
                                ctx,
                                original_query,
                                expr.loc.clone(),
                                v.inner().as_str(),
                            );

                            let field_name = v.inner().clone();
                            let (rust_type, fields) = type_to_rust_string_and_fields(&inferred_type, &traversal.should_collect, ctx, &field_name);

                            query.return_values.push((
                                field_name,
                                ReturnValue {
                                    name: rust_type,
                                    fields,
                                    literal_value: None,
                                }
                            ));
                        }
                        _ => {
                            let field_name = "data".to_string();
                            let (rust_type, fields) = type_to_rust_string_and_fields(&inferred_type, &traversal.should_collect, ctx, &field_name);

                            query.return_values.push((
                                field_name,
                                ReturnValue {
                                    name: rust_type,
                                    fields,
                                    literal_value: None,
                                }
                            ));
                        }
                    }
                }
                GeneratedStatement::Identifier(id) => {
                    is_valid_identifier(ctx, original_query, expr.loc.clone(), id.inner().as_str());
                    let identifier_end_type = match scope.get(id.inner().as_str()) {
                        Some(var_info) => var_info.ty.clone(),
                        None => {
                            generate_error!(
                                ctx,
                                original_query,
                                expr.loc.clone(),
                                E301,
                                id.inner().as_str()
                            );
                            Type::Unknown
                        }
                    };

                    let field_name = id.inner().clone();
                    let (rust_type, fields) = type_to_rust_string_and_fields(&identifier_end_type, &ShouldCollect::No, ctx, &field_name);

                    query.return_values.push((
                        field_name,
                        ReturnValue {
                            name: rust_type,
                            fields,
                            literal_value: None,
                        }
                    ));
                }
                GeneratedStatement::Literal(l) => {
                    let field_name = "data".to_string();
                    let rust_type = "Value".to_string();

                    query.return_values.push((
                        field_name,
                        ReturnValue {
                            name: rust_type,
                            fields: vec![],
                            literal_value: Some(l.clone()),
                        }
                    ));
                }
                GeneratedStatement::Empty => query.return_values = vec![],

                // given all erroneous statements are caught by the analyzer, this should never happen
                // all malformed statements (not gramatically correct) should be caught by the parser
                _ => unreachable!(),
            }
        }
        ReturnType::Array(values) => {
            // For arrays, check if they contain simple expressions (identifiers/traversals)
            // or complex nested structures
            let is_simple_array = values.iter().all(|v| matches!(v, ReturnType::Expression(_)));

            if is_simple_array {
                // Process each element as a separate return value
                for return_expr in values {
                    analyze_return_expr(ctx, original_query, scope, query, return_expr);
                }
            } else {
                // Complex nested array/object structure - not yet supported
                // TODO: Implement proper nested structure serialization
                // For now, this will result in an empty return which the user needs to handle manually
            }
        }
        ReturnType::Object(values) => {
            // Check if this is a simple object with only expression values
            let is_simple_object = values.values().all(|v| matches!(v, ReturnType::Expression(_)));

            if is_simple_object {
                // Process each field in the object
                for (_field_name, return_expr) in values {
                    // Recursively analyze each field's return expression
                    analyze_return_expr(ctx, original_query, scope, query, return_expr);
                }
            } else {
                // Complex nested object - not yet supported
                // TODO: Implement proper nested structure serialization
                // For now, this will result in an empty return which the user needs to handle manually
            }
        }
        ReturnType::Empty => {}
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::helixc::parser::{write_to_temp_file, HelixParser};

    // ============================================================================
    // Parameter Validation Tests
    // ============================================================================

    #[test]
    fn test_unknown_parameter_type() {
        let source = r#"
            N::Person { name: String }

            QUERY test(data: UnknownType) =>
                p <- N<Person>
                RETURN p
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(diagnostics.iter().any(|d| d.error_code == ErrorCode::E209));
        assert!(diagnostics.iter().any(|d| d.message.contains("unknown type") && d.message.contains("UnknownType")));
    }

    #[test]
    fn test_valid_array_parameter_type() {
        let source = r#"
            N::Person { name: String }

            QUERY createPeople(names: [String]) =>
                p <- N<Person>
                RETURN p
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        // Should not have E209 errors for valid array parameter type
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E209));
    }

    // ============================================================================
    // Variable Scope Tests
    // ============================================================================

    #[test]
    fn test_variable_not_in_scope() {
        let source = r#"
            N::Person { name: String }

            QUERY test() =>
                p <- N<Person>
                RETURN unknownVar
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
        assert!(diagnostics.iter().any(|d| d.message.contains("not in scope") && d.message.contains("unknownVar")));
    }

    #[test]
    fn test_parameter_in_scope() {
        let source = r#"
            N::Person { name: String }

            QUERY test(id: ID) =>
                p <- N<Person>(id)
                RETURN p
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    #[test]
    fn test_assigned_variable_in_scope() {
        let source = r#"
            N::Person { name: String }

            QUERY test() =>
                p <- N<Person>
                result <- p::{name}
                RETURN result
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    // ============================================================================
    // MCP Macro Validation Tests
    // ============================================================================

    #[test]
    fn test_mcp_query_single_return_valid() {
        let source = r#"
            N::Person { name: String }

            #[mcp]
            QUERY getPerson(id: ID) =>
                person <- N<Person>(id)
                RETURN person
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E401));
    }

    #[test]
    fn test_mcp_query_multiple_returns_invalid() {
        let source = r#"
            N::Person { name: String }

            #[mcp]
            QUERY getPerson() =>
                p1 <- N<Person>
                p2 <- N<Person>
                RETURN p1, p2
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(diagnostics.iter().any(|d| d.error_code == ErrorCode::E401));
        assert!(diagnostics.iter().any(|d| d.message.contains("MCP query must return a single value")));
    }

    #[test]
    fn test_non_mcp_query_multiple_returns_valid() {
        let source = r#"
            N::Person { name: String }

            QUERY getPeople() =>
                p1 <- N<Person>
                p2 <- N<Person>
                RETURN p1, p2
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        // Non-MCP queries can return multiple values
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E401));
    }

    // ============================================================================
    // Return Value Tests
    // ============================================================================

    #[test]
    fn test_return_literal_value() {
        let source = r#"
            N::Person { name: String }

            QUERY test() =>
                p <- N<Person>
                RETURN "success"
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        // Should not have errors for returning literal
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    #[test]
    fn test_return_multiple_values() {
        let source = r#"
            N::Person { name: String }

            QUERY test() =>
                p1 <- N<Person>
                p2 <- N<Person>
                RETURN p1, p2
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    #[test]
    fn test_return_object() {
        let source = r#"
            N::Person { name: String }

            QUERY test() =>
                p <- N<Person>
                RETURN {person: p, status: "found"}
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    // ============================================================================
    // Model Macro Tests
    // ============================================================================

    #[test]
    fn test_model_macro_sets_embedding_model() {
        let source = r#"
            V::Document { content: String, embedding: [F32] }

            #[model("gpt-4")]
            QUERY addDoc(text: String) =>
                doc <- AddV<Document>(Embed(text), {content: text})
                RETURN doc
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, generated) = result.unwrap();
        // Model macro should be processed without errors
        assert!(diagnostics.is_empty() || !diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));

        // Check that the generated query has the embedding model set
        assert_eq!(generated.queries.len(), 1);
        // Model name includes quotes from parsing
        assert_eq!(generated.queries[0].embedding_model_to_use, Some("\"gpt-4\"".to_string()));
    }

    // ============================================================================
    // Complex Query Tests
    // ============================================================================

    #[test]
    fn test_query_with_traversal_and_filtering() {
        let source = r#"
            N::Person { name: String, age: U32 }
            E::Knows { From: Person, To: Person }

            QUERY getFriends(id: ID, minAge: U32) =>
                person <- N<Person>(id)
                friends <- person::Out<Knows>::WHERE(_::{age}::GT(minAge))
                RETURN friends
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        // Complex queries should not have scope errors
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    #[test]
    fn test_query_with_multiple_assignments() {
        let source = r#"
            N::Person { name: String }
            N::Company { name: String }
            E::WorksAt { From: Person, To: Company }

            QUERY getEmployees(companyId: ID) =>
                company <- N<Company>(companyId)
                edges <- company::InE<WorksAt>
                people <- edges::FromN
                RETURN people
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }

    #[test]
    fn test_query_returning_property_access() {
        let source = r#"
            N::Person { name: String, email: String }

            QUERY getEmail(id: ID) =>
                person <- N<Person>(id)
                email <- person::{email}
                RETURN email
        "#;

        let content = write_to_temp_file(vec![source]);
        let parsed = HelixParser::parse_source(&content).unwrap();
        let result = crate::helixc::analyzer::analyze(&parsed);

        assert!(result.is_ok());
        let (diagnostics, _) = result.unwrap();
        assert!(!diagnostics.iter().any(|d| d.error_code == ErrorCode::E301));
    }
}
