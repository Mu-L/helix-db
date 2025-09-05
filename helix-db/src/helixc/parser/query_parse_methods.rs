use crate::helixc::parser::{
    HelixParser, Rule,
    location::HasLoc,
    parser_methods::ParserError,
    types::{BuiltInMacro, Parameter, Query, Statement, StatementType},
};
use pest::iterators::Pair;
use std::collections::HashSet;

impl HelixParser {
    pub(super) fn parse_query_def(
        &self,
        pair: Pair<Rule>,
        filepath: String,
    ) -> Result<Query, ParserError> {
        let original_query = pair.clone().as_str().to_string();
        let mut pairs = pair.clone().into_inner();
        let built_in_macro = match pairs.peek() {
            Some(pair) if pair.as_rule() == Rule::built_in_macro => {
                let built_in_macro = match pair.into_inner().next() {
                    Some(pair) => match pair.as_rule() {
                        Rule::mcp_macro => Some(BuiltInMacro::MCP),
                        Rule::model_macro => Some(BuiltInMacro::Model(
                            pair.into_inner().next().unwrap().as_str().to_string(),
                        )),
                        _ => None,
                    },
                    _ => None,
                };
                pairs.next();
                built_in_macro
            }
            _ => None,
        };
        let name = pairs.next().unwrap().as_str().to_string();
        let parameters = self.parse_parameters(pairs.next().unwrap())?;
        let body = pairs.next().unwrap();
        let statements = self.parse_query_body(body)?;
        let return_values = self.parse_return_statement(pairs.next().unwrap())?;

        Ok(Query {
            built_in_macro,
            name,
            parameters,
            statements,
            return_values,
            original_query,
            loc: pair.loc_with_filepath(filepath),
        })
    }

    pub(super) fn parse_parameters(&self, pair: Pair<Rule>) -> Result<Vec<Parameter>, ParserError> {
        let mut seen = HashSet::new();
        pair.clone()
            .into_inner()
            .map(|p: Pair<'_, Rule>| -> Result<Parameter, ParserError> {
                let mut inner = p.into_inner();
                let name = {
                    let pair = inner.next().unwrap();
                    (pair.loc(), pair.as_str().to_string())
                };

                // gets optional param
                let is_optional = inner
                    .peek()
                    .is_some_and(|p| p.as_rule() == Rule::optional_param);
                if is_optional {
                    inner.next();
                }

                // gets param type
                let param_type_pair = inner
                    .clone()
                    .next()
                    .unwrap()
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap();
                let param_type_location = param_type_pair.loc();
                let param_type = self.parse_field_type(
                    // unwraps the param type to get the rule (array, object, named_type, etc)
                    param_type_pair,
                    Some(&self.source),
                )?;

                if seen.insert(name.1.clone()) {
                    Ok(Parameter {
                        name,
                        param_type: (param_type_location, param_type),
                        is_optional,
                        loc: pair.loc(),
                    })
                } else {
                    Err(ParserError::from(format!(
                        r#"Duplicate parameter name: {}
                            Please use unique parameter names.

                            Error happened at line {} column {} here: {}
                        "#,
                        name.1,
                        pair.line_col().0,
                        pair.line_col().1,
                        pair.as_str(),
                    )))
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub(super) fn parse_query_body(&self, pair: Pair<Rule>) -> Result<Vec<Statement>, ParserError> {
        pair.into_inner()
            .map(|p| match p.as_rule() {
                Rule::get_stmt => Ok(Statement {
                    loc: p.loc(),
                    statement: StatementType::Assignment(self.parse_assignment(p)?),
                }),
                Rule::creation_stmt => Ok(Statement {
                    loc: p.loc(),
                    statement: StatementType::Expression(self.parse_expression(p)?),
                }),

                Rule::drop => {
                    let inner = p.into_inner().next().unwrap();
                    Ok(Statement {
                        loc: inner.loc(),
                        statement: StatementType::Drop(self.parse_expression(inner)?),
                    })
                }

                Rule::for_loop => Ok(Statement {
                    loc: p.loc(),
                    statement: StatementType::ForLoop(self.parse_for_loop(p)?),
                }),
                _ => Err(ParserError::from(format!(
                    "Unexpected statement type in query body: {:?}",
                    p.as_rule()
                ))),
            })
            .collect()
    }
}
