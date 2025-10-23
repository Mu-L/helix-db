use crate::helixc::generator::utils::{write_properties, write_properties_slice, VecData};

use super::{
    bool_ops::{BoExp, BoolOp},
    source_steps::SourceStep,
    utils::{GenRef, GeneratedValue, Order, Separator},
};
use core::fmt;
use std::fmt::{Debug, Display};

#[derive(Clone)]
pub enum TraversalType {
    FromSingle(GenRef<String>),
    FromIter(GenRef<String>),
    Ref,
    Mut,
    Empty,
    Update(Option<Vec<(String, GeneratedValue)>>),
}
impl Debug for TraversalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraversalType::FromSingle(_) => write!(f, "FromSingle"),
            TraversalType::FromIter(_) => write!(f, "FromIter"),
            TraversalType::Ref => write!(f, "Ref"),
            _ => write!(f, "other"),
        }
    }
}
// impl Display for TraversalType {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             TraversalType::FromVar => write!(f, ""),
//             TraversalType::Ref => write!(f, "G::new(Arc::clone(&db), &txn)"),

//             TraversalType::Mut => write!(f, "G::new_mut(Arc::clone(&db), &mut txn)"),
//             TraversalType::Nested(nested) => {
//                 assert!(nested.inner().len() > 0, "Empty nested traversal name");
//                 write!(f, "G::new_from(Arc::clone(&db), &txn, {})", nested)
//             }
//             TraversalType::Update => write!(f, ""),
//             // TraversalType::FromVar(var) => write!(f, "G::new_from(Arc::clone(&db), &txn, {})", var),
//             TraversalType::Empty => panic!("Should not be empty"),
//         }
//     }
// }
#[derive(Clone)]
pub enum ShouldCollect {
    ToVec,
    ToObj,
    No,
    Try,
    ToValue,
}
impl Display for ShouldCollect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShouldCollect::ToVec => write!(f, ".collect_to::<Vec<_>>()"),
            ShouldCollect::ToObj => write!(f, ".collect_to_obj()"),
            ShouldCollect::Try => write!(f, "?"),
            ShouldCollect::No => write!(f, ""),
            ShouldCollect::ToValue => write!(f, ".collect_to_value()"),
        }
    }
}

#[derive(Clone)]
pub struct Traversal {
    pub traversal_type: TraversalType,
    pub source_step: Separator<SourceStep>,
    pub steps: Vec<Separator<Step>>,
    pub should_collect: ShouldCollect,
}

impl Display for Traversal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.traversal_type {
            TraversalType::FromSingle(var) => {
                write!(f, "G::from_iter(&db, &txn, std::iter::once({var}.clone()), &arena)")?;
                write!(f, "{}", self.source_step)?;
                for step in &self.steps {
                    write!(f, "\n{step}")?;
                }
            }
            TraversalType::FromIter(var) => {
                write!(f, "G::from_iter(&db, &txn, {var}.iter().cloned(), &arena)")?;
                write!(f, "{}", self.source_step)?;
                for step in &self.steps {
                    write!(f, "\n{step}")?;
                }
            }
            TraversalType::Ref => {
                write!(f, "G::new(&db, &txn, &arena)")?;
                write!(f, "{}", self.source_step)?;
                for step in &self.steps {
                    write!(f, "\n{step}")?;
                }
            }

            TraversalType::Mut => {
                write!(f, "G::new_mut(&db, &arena, &mut txn)")?;
                write!(f, "{}", self.source_step)?;
                for step in &self.steps {
                    write!(f, "\n{step}")?;
                }
            }

            TraversalType::Empty => panic!("Should not be empty"),
            TraversalType::Update(properties) => {
                write!(f, "{{")?;
                write!(f, "let update_tr = G::new(&db, &txn, &arena)")?;
                write!(f, "{}", self.source_step)?;
                for step in &self.steps {
                    write!(f, "\n{step}")?;
                }
                write!(f, "\n    .collect_to::<Vec<_>>();")?;
                write!(
                    f,
                    "G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)",
                )?;
                write!(f, "\n    .update({})", write_properties_slice(properties))?;
                write!(f, "\n    .collect_to_obj()")?;
                write!(f, "}}")?;
            }
        }
        write!(f, "{}", self.should_collect)
    }
}
impl Default for Traversal {
    fn default() -> Self {
        Self {
            traversal_type: TraversalType::Ref,
            source_step: Separator::Empty(SourceStep::Empty),
            steps: vec![],
            should_collect: ShouldCollect::ToVec,
        }
    }
}
#[derive(Clone)]
pub enum Step {
    // graph steps
    Out(Out),
    In(In),
    OutE(OutE),
    InE(InE),
    FromN,
    ToN,
    FromV(FromV),
    ToV(ToV),

    // utils
    Count,

    Where(Where),
    Range(Range),
    OrderBy(OrderBy),
    Dedup,

    // bool ops
    BoolOp(BoolOp),

    // property
    PropertyFetch(GenRef<String>),

    // closure
    // Closure(ClosureRemapping),

    // shortest path
    ShortestPath(ShortestPath),
    ShortestPathDijkstras(ShortestPathDijkstras),
    ShortestPathBFS(ShortestPathBFS),

    // search vector
    SearchVector(SearchVectorStep),

    GroupBy(GroupBy),

    AggregateBy(AggregateBy),
}
impl Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Count => write!(f, "count_to_val()"),
            Step::Dedup => write!(f, "dedup()"),
            Step::FromN => write!(f, "from_n()"),
            Step::FromV(from_v) => write!(f, "{from_v}"),
            Step::ToN => write!(f, "to_n()"),
            Step::ToV(to_v) => write!(f, "{to_v}"),
            Step::PropertyFetch(property) => write!(f, "get_property({property})"),

            Step::Out(out) => write!(f, "{out}"),
            Step::In(in_) => write!(f, "{in_}"),
            Step::OutE(out_e) => write!(f, "{out_e}"),
            Step::InE(in_e) => write!(f, "{in_e}"),
            Step::Where(where_) => write!(f, "{where_}"),
            Step::Range(range) => write!(f, "{range}"),
            Step::OrderBy(order_by) => write!(f, "{order_by}"),
            Step::BoolOp(bool_op) => write!(f, "{bool_op}"),
            Step::ShortestPath(shortest_path) => write!(f, "{shortest_path}"),
            Step::ShortestPathDijkstras(shortest_path_dijkstras) => {
                write!(f, "{shortest_path_dijkstras}")
            }
            Step::ShortestPathBFS(shortest_path_bfs) => write!(f, "{shortest_path_bfs}"),
            Step::SearchVector(search_vector) => write!(f, "{search_vector}"),
            Step::GroupBy(group_by) => write!(f, "{group_by}"),
            Step::AggregateBy(aggregate_by) => write!(f, "{aggregate_by}"),
        }
    }
}
impl Debug for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Count => write!(f, "Count"),
            Step::Dedup => write!(f, "Dedup"),
            Step::FromN => write!(f, "FromN"),
            Step::ToN => write!(f, "ToN"),
            Step::PropertyFetch(property) => write!(f, "get_property({property})"),
            Step::FromV(_) => write!(f, "FromV"),
            Step::ToV(_) => write!(f, "ToV"),
            Step::Out(_) => write!(f, "Out"),
            Step::In(_) => write!(f, "In"),
            Step::OutE(_) => write!(f, "OutE"),
            Step::InE(_) => write!(f, "InE"),
            Step::Where(_) => write!(f, "Where"),
            Step::Range(_) => write!(f, "Range"),
            Step::OrderBy(_) => write!(f, "OrderBy"),
            Step::BoolOp(_) => write!(f, "Bool"),
            Step::ShortestPath(_) => write!(f, "ShortestPath"),
            Step::ShortestPathDijkstras(_) => write!(f, "ShortestPathDijkstras"),
            Step::ShortestPathBFS(_) => write!(f, "ShortestPathBFS"),
            Step::SearchVector(_) => write!(f, "SearchVector"),
            Step::GroupBy(_) => write!(f, "GroupBy"),
            Step::AggregateBy(_) => write!(f, "AggregateBy"),
        }
    }
}

#[derive(Clone)]
pub struct FromV {
    pub get_vector_data: bool,
}
impl Display for FromV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "from_v({})", self.get_vector_data)
    }
}

#[derive(Clone)]
pub struct ToV {
    pub get_vector_data: bool,
}
impl Display for ToV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "to_v({})", self.get_vector_data)
    }
}

#[derive(Clone, PartialEq)]
pub enum EdgeType {
    Node,
    Vec,
}

impl Display for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeType::Node => write!(f, "node"),
            EdgeType::Vec => write!(f, "vec"),
        }
    }
}

#[derive(Clone)]
pub struct Out {
    pub label: GenRef<String>,
    pub edge_type: EdgeType,
    pub get_vector_data: bool,
}
impl Display for Out {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.edge_type {
            EdgeType::Node => write!(f, "out_node({})", self.label),
            EdgeType::Vec => write!(f, "out_vec({}, {})", self.label, self.get_vector_data),
        }
    }
}

#[derive(Clone)]
pub struct In {
    pub label: GenRef<String>,
    pub edge_type: EdgeType,
    pub get_vector_data: bool,
}
impl Display for In {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.edge_type {
            EdgeType::Node => write!(f, "in_node({})", self.label),
            EdgeType::Vec => write!(f, "in_vec({}, {})", self.label, self.get_vector_data),
        }
    }
}

#[derive(Clone)]
pub struct OutE {
    pub label: GenRef<String>,
}
impl Display for OutE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "out_e({})", self.label)
    }
}

#[derive(Clone)]
pub struct InE {
    pub label: GenRef<String>,
}
impl Display for InE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "in_e({})", self.label)
    }
}

#[derive(Clone)]
pub enum Where {
    Ref(WhereRef),
}
impl Display for Where {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Where::Ref(wr) = self;
        write!(f, "{wr}")
    }
}

#[derive(Clone)]
pub struct WhereRef {
    pub expr: BoExp,
}
impl Display for WhereRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Check if this is a simple property check that can be optimized
        if let BoExp::Expr(traversal) = &self.expr {
            if let TraversalType::FromSingle(var) = &traversal.traversal_type {
                // Check if the variable is "val"
                let is_val = matches!(var, GenRef::Std(s) | GenRef::Literal(s) if s == "val");

                if is_val && traversal.steps.len() == 2 {
                    // Check if we have PropertyFetch followed by BoolOp
                    let mut prop: Option<&GenRef<String>> = None;
                    let mut bool_op: Option<&BoolOp> = None;

                    for step in &traversal.steps {
                        match step {
                            Separator::Period(Step::PropertyFetch(p))
                            | Separator::Newline(Step::PropertyFetch(p))
                            | Separator::Empty(Step::PropertyFetch(p)) => prop = Some(p),
                            Separator::Period(Step::BoolOp(op))
                            | Separator::Newline(Step::BoolOp(op))
                            | Separator::Empty(Step::BoolOp(op)) => bool_op = Some(op),
                            _ => {}
                        }
                    }

                    // If we found both PropertyFetch and BoolOp, generate optimized code
                    if let (Some(prop), Some(bool_op)) = (prop, bool_op) {
                        let bool_expr = match bool_op {
                            BoolOp::Gt(gt) => format!("*v{gt}"),
                            BoolOp::Gte(gte) => format!("*v{gte}"),
                            BoolOp::Lt(lt) => format!("*v{lt}"),
                            BoolOp::Lte(lte) => format!("*v{lte}"),
                            BoolOp::Eq(eq) => format!("*v{eq}"),
                            BoolOp::Neq(neq) => format!("*v{neq}"),
                            BoolOp::Contains(contains) => format!("v{contains}"),
                            BoolOp::IsIn(is_in) => format!("v{is_in}"),
                        };
                        return write!(
                            f,
                            "filter_ref(|val, txn|{{
                if let Ok(val) = val {{
                    Ok(val
                    .get_property({})
                    .map_or(false, |v| {}))
                }} else {{
                    Ok(false)
                }}
            }})",
                            prop, bool_expr
                        );
                    }
                }
            }
        }

        // Fall back to default (unoptimized) code generation
        write!(
            f,
            "filter_ref(|val, txn|{{
                if let Ok(val) = val {{
                    Ok({})
                }} else {{
                    Ok(false)
                }}
            }})",
            self.expr
        )
    }
}

#[derive(Clone)]
pub struct Range {
    pub start: GeneratedValue,
    pub end: GeneratedValue,
}
impl Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "range({}, {})", self.start, self.end)
    }
}

#[derive(Clone)]
pub struct OrderBy {
    pub property: GenRef<String>,
    pub order: Order,
}
impl Display for OrderBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.order {
            Order::Asc => write!(f, "order_by_asc({})", self.property),
            Order::Desc => write!(f, "order_by_desc({})", self.property),
        }
    }
}

#[derive(Clone)]
pub struct GroupBy {
    pub should_count: bool,
    pub properties: Vec<GenRef<String>>,
}
impl Display for GroupBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "group_by(&[{}], {})",
            self.properties
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.should_count
        )
    }
}

#[derive(Clone)]
pub struct AggregateBy {
    pub should_count: bool,
    pub properties: Vec<GenRef<String>>,
}
impl Display for AggregateBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "aggregate_by(&[{}], {})",
            self.properties
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.should_count
        )
    }
}

#[derive(Clone)]
pub struct ShortestPath {
    pub label: Option<GenRef<String>>,
    pub from: Option<GenRef<String>>,
    pub to: Option<GenRef<String>>,
    pub algorithm: Option<PathAlgorithm>,
}

#[derive(Clone)]
pub struct ShortestPathDijkstras {
    pub label: Option<GenRef<String>>,
    pub from: Option<GenRef<String>>,
    pub to: Option<GenRef<String>>,
    pub weight_property: Option<GenRef<String>>,
}

#[derive(Clone)]
pub struct ShortestPathBFS {
    pub label: Option<GenRef<String>>,
    pub from: Option<GenRef<String>>,
    pub to: Option<GenRef<String>>,
}

#[derive(Clone)]
pub enum PathAlgorithm {
    BFS,
    Dijkstra,
}
impl Display for ShortestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.algorithm {
            Some(PathAlgorithm::Dijkstra) => {
                write!(
                    f,
                    "shortest_path_with_algorithm({}, {}, {}, PathAlgorithm::Dijkstra)",
                    self.label
                        .as_ref()
                        .map_or("None".to_string(), |label| format!("Some({label})")),
                    self.from
                        .as_ref()
                        .map_or("None".to_string(), |from| format!("Some(&{from})")),
                    self.to
                        .as_ref()
                        .map_or("None".to_string(), |to| format!("Some(&{to})"))
                )
            }
            Some(PathAlgorithm::BFS) => {
                write!(
                    f,
                    "shortest_path_with_algorithm({}, {}, {}, PathAlgorithm::BFS)",
                    self.label
                        .as_ref()
                        .map_or("None".to_string(), |label| format!("Some({label})")),
                    self.from
                        .as_ref()
                        .map_or("None".to_string(), |from| format!("Some(&{from})")),
                    self.to
                        .as_ref()
                        .map_or("None".to_string(), |to| format!("Some(&{to})"))
                )
            }
            None => {
                // Default to BFS for backward compatibility
                write!(
                    f,
                    "shortest_path({}, {}, {})",
                    self.label
                        .as_ref()
                        .map_or("None".to_string(), |label| format!("Some({label})")),
                    self.from
                        .as_ref()
                        .map_or("None".to_string(), |from| format!("Some(&{from})")),
                    self.to
                        .as_ref()
                        .map_or("None".to_string(), |to| format!("Some(&{to})"))
                )
            }
        }
    }
}

impl Display for ShortestPathDijkstras {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "shortest_path_with_algorithm({}, {}, {}, PathAlgorithm::Dijkstra)",
            self.label
                .as_ref()
                .map_or("None".to_string(), |label| format!("Some({label})")),
            self.from
                .as_ref()
                .map_or("None".to_string(), |from| format!("Some(&{from})")),
            self.to
                .as_ref()
                .map_or("None".to_string(), |to| format!("Some(&{to})"))
        )
    }
}

impl Display for ShortestPathBFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "shortest_path_with_algorithm({}, {}, {}, PathAlgorithm::BFS)",
            self.label
                .as_ref()
                .map_or("None".to_string(), |label| format!("Some({label})")),
            self.from
                .as_ref()
                .map_or("None".to_string(), |from| format!("Some(&{from})")),
            self.to
                .as_ref()
                .map_or("None".to_string(), |to| format!("Some(&{to})"))
        )
    }
}

#[derive(Clone)]
pub struct SearchVectorStep {
    pub vec: VecData,
    pub k: GeneratedValue,
}
impl Display for SearchVectorStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "brute_force_search_v({}, {})", self.vec, self.k)
    }
}
