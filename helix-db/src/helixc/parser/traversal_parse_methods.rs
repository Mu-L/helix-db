use crate::{
    helixc::parser::{
        HelixParser, Rule,
        location::HasLoc,
        ParserError,
        types::{
            FieldAddition, FieldValue, FieldValueType, GraphStep, GraphStepType, IdType, Object,
            ShortestPath, StartNode, Step, StepType, Traversal, ValueType,
        },
    },
    protocol::value::Value,
};
use pest::iterators::{Pair, Pairs};

impl HelixParser {
    pub(super) fn parse_traversal(&self, pair: Pair<Rule>) -> Result<Traversal, ParserError> {
        let mut pairs = pair.clone().into_inner();
        let start = self.parse_start_node(pairs.next().unwrap())?;
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
            _ => Ok(StartNode::Anonymous),
        }
    }

    pub(super) fn parse_step(&self, pair: Pair<Rule>) -> Result<Step, ParserError> {
        let inner = pair.clone().into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::graph_step => Ok(Step {
                loc: inner.loc(),
                step: StepType::Node(self.parse_graph_step(inner)),
            }),
            Rule::object_step => Ok(Step {
                loc: inner.loc(),
                step: StepType::Object(self.parse_object_step(inner)?),
            }),
            Rule::closure_step => Ok(Step {
                loc: inner.loc(),
                step: StepType::Closure(self.parse_closure(inner)?),
            }),
            Rule::where_step => Ok(Step {
                loc: inner.loc(),
                step: StepType::Where(Box::new(self.parse_expression(inner)?)),
            }),
            Rule::range_step => Ok(Step {
                loc: inner.loc(),
                step: StepType::Range(self.parse_range(pair)?),
            }),

            Rule::bool_operations => Ok(Step {
                loc: inner.loc(),
                step: StepType::BooleanOperation(self.parse_bool_operation(inner)?),
            }),
            Rule::count => Ok(Step {
                loc: inner.loc(),
                step: StepType::Count,
            }),
            Rule::ID => Ok(Step {
                loc: inner.loc(),
                step: StepType::Object(Object {
                    fields: vec![FieldAddition {
                        key: "id".to_string(),
                        value: FieldValue {
                            loc: pair.loc(),
                            value: FieldValueType::Identifier("id".to_string()),
                        },
                        loc: pair.loc(),
                    }],
                    should_spread: false,
                    loc: pair.loc(),
                }),
            }),
            Rule::update => Ok(Step {
                loc: inner.loc(),
                step: StepType::Update(self.parse_update(inner)?),
            }),
            Rule::exclude_field => Ok(Step {
                loc: inner.loc(),
                step: StepType::Exclude(self.parse_exclude(inner)?),
            }),
            Rule::AddE => Ok(Step {
                loc: inner.loc(),
                step: StepType::AddEdge(self.parse_add_edge(inner, true)?),
            }),
            Rule::order_by => Ok(Step {
                loc: inner.loc(),
                step: StepType::OrderBy(self.parse_order_by(inner)?),
            }),
            _ => Err(ParserError::from(format!(
                "Unexpected step type: {:?}",
                inner.as_rule()
            ))),
        }
    }

    pub(super) fn parse_graph_step(&self, pair: Pair<Rule>) -> GraphStep {
        let types = |pair: &Pair<Rule>| {
            pair.clone()
                .into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .ok_or_else(|| ParserError::from("Expected type".to_string()))
                .unwrap()
        };
        let pair = pair.into_inner().next().unwrap();
        match pair.as_rule() {
            Rule::out_e => {
                let types = types(&pair);
                GraphStep {
                    loc: pair.loc(),
                    step: GraphStepType::OutE(types),
                }
            }
            Rule::in_e => {
                let types = types(&pair);
                GraphStep {
                    loc: pair.loc(),
                    step: GraphStepType::InE(types),
                }
            }
            Rule::from_n => GraphStep {
                loc: pair.loc(),
                step: GraphStepType::FromN,
            },
            Rule::to_n => GraphStep {
                loc: pair.loc(),
                step: GraphStepType::ToN,
            },
            Rule::from_v => GraphStep {
                loc: pair.loc(),
                step: GraphStepType::FromV,
            },
            Rule::to_v => GraphStep {
                loc: pair.loc(),
                step: GraphStepType::ToV,
            },
            Rule::out => {
                let types = types(&pair);
                GraphStep {
                    loc: pair.loc(),
                    step: GraphStepType::Out(types),
                }
            }
            Rule::in_nodes => {
                let types = types(&pair);
                GraphStep {
                    loc: pair.loc(),
                    step: GraphStepType::In(types),
                }
            }
            Rule::shortest_path => {
                let (type_arg, from, to) = pair.clone().into_inner().fold(
                    (None, None, None),
                    |(type_arg, from, to), p| match p.as_rule() {
                        Rule::type_args => (
                            Some(p.into_inner().next().unwrap().as_str().to_string()),
                            from,
                            to,
                        ),
                        Rule::to_from => match p.into_inner().next() {
                            Some(p) => match p.as_rule() {
                                Rule::to => (
                                    type_arg,
                                    from,
                                    Some(p.into_inner().next().unwrap().as_str().to_string()),
                                ),
                                Rule::from => (
                                    type_arg,
                                    Some(p.into_inner().next().unwrap().as_str().to_string()),
                                    to,
                                ),
                                _ => unreachable!(),
                            },
                            None => (type_arg, from, to),
                        },
                        _ => (type_arg, from, to),
                    },
                );
                GraphStep {
                    loc: pair.loc(),
                    step: GraphStepType::ShortestPath(ShortestPath {
                        loc: pair.loc(),
                        from: from.map(|id| IdType::Identifier {
                            value: id,
                            loc: pair.loc(),
                        }),
                        to: to.map(|id| IdType::Identifier {
                            value: id,
                            loc: pair.loc(),
                        }),
                        type_arg,
                    }),
                }
            }
            Rule::search_vector => GraphStep {
                loc: pair.loc(),
                step: GraphStepType::SearchVector(self.parse_search_vector(pair).unwrap()),
            },
            _ => {
                unreachable!()
            }
        }
    }
}
