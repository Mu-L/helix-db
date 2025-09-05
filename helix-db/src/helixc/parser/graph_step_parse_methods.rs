use crate::helixc::parser::{
    HelixParser, Rule,
    location::HasLoc,
    parser_methods::ParserError,
    types::{
        BooleanOp, BooleanOpType, Closure, Exclude, Expression, FieldAddition, FieldValue,
        FieldValueType, Object, OrderBy, OrderByType, Update,
    },
};
use pest::iterators::Pair;

impl HelixParser {
    pub fn parse_order_by(&self, pair: Pair<Rule>) -> Result<OrderBy, ParserError> {
        let mut inner = pair.clone().into_inner();
        let order_by_type = match inner.next().unwrap().into_inner().next().unwrap().as_rule() {
            Rule::asc => OrderByType::Asc,
            Rule::desc => OrderByType::Desc,
            _ => unreachable!(),
        };
        let expression = self.parse_expression(inner.next().unwrap())?;
        Ok(OrderBy {
            loc: pair.loc(),
            order_by_type,
            expression: Box::new(expression),
        })
    }

    pub fn parse_range(&self, pair: Pair<Rule>) -> Result<(Expression, Expression), ParserError> {
        let mut inner = pair.into_inner().next().unwrap().into_inner();
        // println!("inner: {:?}", inner);
        let start = self.parse_expression(inner.next().unwrap())?;
        let end = self.parse_expression(inner.next().unwrap())?;

        Ok((start, end))
    }

    pub fn parse_bool_operation(&self, pair: Pair<Rule>) -> Result<BooleanOp, ParserError> {
        let inner = pair.clone().into_inner().next().unwrap();
        let expr = match inner.as_rule() {
            Rule::GT => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::GreaterThan(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::GTE => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::GreaterThanOrEqual(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::LT => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::LessThan(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::LTE => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::LessThanOrEqual(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::EQ => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::Equal(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::NEQ => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::NotEqual(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::CONTAINS => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::Contains(Box::new(
                    self.parse_expression(inner.into_inner().next().unwrap())?,
                )),
            },
            Rule::IS_IN => BooleanOp {
                loc: pair.loc(),
                op: BooleanOpType::IsIn(Box::new(self.parse_expression(inner)?)),
            },
            _ => return Err(ParserError::from("Invalid boolean operation")),
        };
        Ok(expr)
    }

    pub fn parse_update(&self, pair: Pair<Rule>) -> Result<Update, ParserError> {
        let fields = self.parse_object_fields(pair.clone())?;
        Ok(Update {
            fields,
            loc: pair.loc(),
        })
    }

    pub fn parse_object_step(&self, pair: Pair<Rule>) -> Result<Object, ParserError> {
        let mut fields = Vec::new();
        let mut should_spread = false;
        for p in pair.clone().into_inner() {
            if p.as_rule() == Rule::spread_object {
                should_spread = true;
                continue;
            }
            let mut pairs = p.clone().into_inner();
            let prop_key = pairs.next().unwrap().as_str().to_string();
            let field_addition = match pairs.next() {
                Some(p) => match p.as_rule() {
                    Rule::evaluates_to_anything => FieldValue {
                        loc: p.loc(),
                        value: FieldValueType::Expression(self.parse_expression(p)?),
                    },
                    Rule::anonymous_traversal => FieldValue {
                        loc: p.loc(),
                        value: FieldValueType::Traversal(Box::new(self.parse_anon_traversal(p)?)),
                    },
                    Rule::mapping_field => FieldValue {
                        loc: p.loc(),
                        value: FieldValueType::Fields(self.parse_object_fields(p)?),
                    },
                    Rule::object_step => FieldValue {
                        loc: p.clone().loc(),
                        value: FieldValueType::Fields(self.parse_object_step(p.clone())?.fields),
                    },
                    _ => self.parse_new_field_value(p)?,
                },
                None if !prop_key.is_empty() => FieldValue {
                    loc: p.loc(),
                    value: FieldValueType::Identifier(prop_key.clone()),
                },
                None => FieldValue {
                    loc: p.loc(),
                    value: FieldValueType::Empty,
                },
            };
            fields.push(FieldAddition {
                loc: p.loc(),
                key: prop_key,
                value: field_addition,
            });
        }
        Ok(Object {
            loc: pair.loc(),
            fields,
            should_spread,
        })
    }

    pub fn parse_closure(&self, pair: Pair<Rule>) -> Result<Closure, ParserError> {
        let mut pairs = pair.clone().into_inner();
        let identifier = pairs.next().unwrap().as_str().to_string();
        let object = self.parse_object_step(pairs.next().unwrap())?;
        Ok(Closure {
            loc: pair.loc(),
            identifier,
            object,
        })
    }

    pub fn parse_exclude(&self, pair: Pair<Rule>) -> Result<Exclude, ParserError> {
        let mut fields = Vec::new();
        for p in pair.clone().into_inner() {
            fields.push((p.loc(), p.as_str().to_string()));
        }
        Ok(Exclude {
            loc: pair.loc(),
            fields,
        })
    }
}
