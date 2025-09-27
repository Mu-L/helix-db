use crate::{
    helixc::parser::{
        HelixParser, ParserError, Rule,
        location::HasLoc,
        types::{IdType, StartNode, Traversal, ValueType},
    },
    protocol::value::Value,
};
use pest::iterators::{Pair, Pairs};

impl HelixParser {
    pub(super) fn parse_traversal(&self, pair: Pair<Rule>) -> Result<Traversal, ParserError> {
        let mut pairs = pair.clone().into_inner();
        let start = self.parse_start_node(pairs.next().ok_or_else(|| ParserError::from(format!("Expected start node, got {pair:?}")))?)?;
        let steps = pairs
            .map(|p| self.parse_step(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Traversal {
            start,
            steps,
            loc: pair.loc(),
        })
    }

    pub(super) fn parse_anon_traversal(&self, pair: Pair<Rule>) -> Result<Traversal, ParserError> {
        let pairs = pair.clone().into_inner();
        let start = StartNode::Anonymous;
        let steps = pairs
            .map(|p| self.parse_step(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Traversal {
            start,
            steps,
            loc: pair.loc(),
        })
    }

    pub(super) fn parse_start_node(&self, pair: Pair<Rule>) -> Result<StartNode, ParserError> {
        match pair.as_rule() {
            Rule::start_node => {
                let pairs = pair.into_inner();
                let mut node_type = String::new();
                let mut ids = None;
                for p in pairs {
                    match p.as_rule() {
                        Rule::type_args => {
                            node_type = p.into_inner().next().unwrap().as_str().to_string();
                            // WATCH
                        }
                        Rule::id_args => {
                            ids = Some(
                                p.into_inner()
                                    .map(|id| {
                                        let id = id.into_inner().next().unwrap();
                                        match id.as_rule() {
                                            Rule::identifier => IdType::Identifier {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            Rule::string_literal => IdType::Literal {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            _ => {
                                                panic!("Should be identifier or string literal")
                                            }
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        Rule::by_index => {
                            ids = Some({
                                let mut pairs: Pairs<'_, Rule> = p.clone().into_inner();
                                let index = match pairs.next().unwrap().clone().into_inner().next()
                                {
                                    Some(id) => match id.as_rule() {
                                        Rule::identifier => IdType::Identifier {
                                            value: id.as_str().to_string(),
                                            loc: id.loc(),
                                        },
                                        Rule::string_literal => IdType::Literal {
                                            value: id.as_str().to_string(),
                                            loc: id.loc(),
                                        },
                                        other => {
                                            panic!(
                                                "Should be identifier or string literal: {other:?}"
                                            )
                                        }
                                    },
                                    None => return Err(ParserError::from("Missing index")),
                                };
                                let value = match pairs.next().unwrap().into_inner().next() {
                                    Some(val) => match val.as_rule() {
                                        Rule::identifier => ValueType::Identifier {
                                            value: val.as_str().to_string(),
                                            loc: val.loc(),
                                        },
                                        Rule::string_literal => ValueType::Literal {
                                            value: Value::from(val.as_str()),
                                            loc: val.loc(),
                                        },
                                        Rule::integer => ValueType::Literal {
                                            value: Value::from(
                                                val.as_str().parse::<i64>().unwrap(),
                                            ),
                                            loc: val.loc(),
                                        },
                                        Rule::float => ValueType::Literal {
                                            value: Value::from(
                                                val.as_str().parse::<f64>().unwrap(),
                                            ),
                                            loc: val.loc(),
                                        },
                                        Rule::boolean => ValueType::Literal {
                                            value: Value::from(
                                                val.as_str().parse::<bool>().unwrap(),
                                            ),
                                            loc: val.loc(),
                                        },
                                        _ => {
                                            panic!("Should be identifier or string literal")
                                        }
                                    },
                                    _ => unreachable!(),
                                };
                                vec![IdType::ByIndex {
                                    index: Box::new(index),
                                    value: Box::new(value),
                                    loc: p.loc(),
                                }]
                            })
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(StartNode::Node { node_type, ids })
            }
            Rule::start_edge => {
                let pairs = pair.into_inner();
                let mut edge_type = String::new();
                let mut ids = None;
                for p in pairs {
                    match p.as_rule() {
                        Rule::type_args => {
                            edge_type = p.into_inner().next().unwrap().as_str().to_string();
                        }
                        Rule::id_args => {
                            ids = Some(
                                p.into_inner()
                                    .map(|id| {
                                        let id = id.into_inner().next().unwrap();
                                        match id.as_rule() {
                                            Rule::identifier => IdType::Identifier {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            Rule::string_literal => IdType::Literal {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            other => {
                                                println!("{other:?}");
                                                panic!("Should be identifier or string literal")
                                            }
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(StartNode::Edge { edge_type, ids })
            }
            Rule::identifier => Ok(StartNode::Identifier(pair.as_str().to_string())),
            Rule::search_vector => Ok(StartNode::SearchVector(self.parse_search_vector(pair)?)),
            Rule::start_vector => {
                let pairs = pair.into_inner();
                let mut vector_type = String::new();
                let mut ids = None;
                for p in pairs {
                    match p.as_rule() {
                        Rule::type_args => {
                            vector_type = p.into_inner().next().unwrap().as_str().to_string();
                        }
                        Rule::id_args => {
                            ids = Some(
                                p.into_inner()
                                    .map(|id| {
                                        let id = id.into_inner().next().unwrap();
                                        match id.as_rule() {
                                            Rule::identifier => IdType::Identifier {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            Rule::string_literal => IdType::Literal {
                                                value: id.as_str().to_string(),
                                                loc: id.loc(),
                                            },
                                            other => {
                                                println!("{other:?}");
                                                panic!("Should be identifier or string literal")
                                            }
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        Rule::by_index => {
                            ids = Some(
                                p.into_inner()
                                    .map(|p| {
                                        let mut pairs = p.clone().into_inner();
                                        let index = match pairs.next().unwrap().into_inner().next()
                                        {
                                            Some(id) => match id.as_rule() {
                                                Rule::identifier => IdType::Identifier {
                                                    value: id.as_str().to_string(),
                                                    loc: id.loc(),
                                                },
                                                Rule::string_literal => IdType::Literal {
                                                    value: id.as_str().to_string(),
                                                    loc: id.loc(),
                                                },
                                                _ => unreachable!(),
                                            },
                                            None => unreachable!(),
                                        };
                                        let value = match pairs.next().unwrap().into_inner().next()
                                        {
                                            Some(val) => match val.as_rule() {
                                                Rule::identifier => ValueType::Identifier {
                                                    value: val.as_str().to_string(),
                                                    loc: val.loc(),
                                                },
                                                Rule::string_literal => ValueType::Literal {
                                                    value: Value::from(val.as_str()),
                                                    loc: val.loc(),
                                                },
                                                Rule::integer => ValueType::Literal {
                                                    value: Value::from(
                                                        val.as_str().parse::<i64>().unwrap(),
                                                    ),
                                                    loc: val.loc(),
                                                },
                                                Rule::float => ValueType::Literal {
                                                    value: Value::from(
                                                        val.as_str().parse::<f64>().unwrap(),
                                                    ),
                                                    loc: val.loc(),
                                                },
                                                Rule::boolean => ValueType::Literal {
                                                    value: Value::from(
                                                        val.as_str().parse::<bool>().unwrap(),
                                                    ),
                                                    loc: val.loc(),
                                                },
                                                _ => {
                                                    panic!("Should be identifier or literal")
                                                }
                                            },
                                            _ => unreachable!(),
                                        };
                                        IdType::ByIndex {
                                            index: Box::new(index),
                                            value: Box::new(value),
                                            loc: p.loc(),
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            );
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(StartNode::Vector { vector_type, ids })
            }
            _ => Ok(StartNode::Anonymous),
        }
    }
}
