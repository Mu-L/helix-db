//! Downstream vector-search pipeline recognition.

use helix_ast::traversal::AstNode;

use super::contract::{NativePipelineOp, NativePipelineOpMatch};
use crate::{context, error, ir, logical, planning};

pub(super) fn pipeline_op_from_ast<'a>(
    ctx: &context::PlannerContext,
    root: &'a AstNode,
) -> Result<NativePipelineOpMatch<'a>, error::PlannerError> {
    Ok(match root {
        AstNode::VectorSearchNodesWithin {
            input,
            label,
            property,
            tenant_value,
            query_vector,
            k,
        } => {
            let search = planning::search::node_vector_search(
                &ctx.indexes,
                label,
                property,
                tenant_value.as_ref(),
                query_vector,
                k,
            )?;
            let ir::NodeAccessPlan::VectorSearch {
                key,
                index,
                query_vector,
                k,
            } = search.plan
            else {
                unreachable!("node vector-search builder returned another access family")
            };
            NativePipelineOpMatch::Op(NativePipelineOp::new(
                input,
                logical::StreamPipelineOp::VectorSearch {
                    plan: Box::new(ir::RestrictedVectorSearchPlan::Nodes {
                        key,
                        index,
                        query_vector,
                        k,
                    }),
                },
            ))
        }
        AstNode::VectorSearchEdgesWithin {
            input,
            label,
            property,
            tenant_value,
            query_vector,
            k,
        } => {
            let search = planning::search::edge_vector_search(
                &ctx.indexes,
                label,
                property,
                tenant_value.as_ref(),
                query_vector,
                k,
            )?;
            let ir::EdgeAccessPlan::VectorSearch {
                key,
                index,
                query_vector,
                k,
            } = search.plan
            else {
                unreachable!("edge vector-search builder returned another access family")
            };
            NativePipelineOpMatch::Op(NativePipelineOp::new(
                input,
                logical::StreamPipelineOp::VectorSearch {
                    plan: Box::new(ir::RestrictedVectorSearchPlan::Edges {
                        key,
                        index,
                        query_vector,
                        k,
                    }),
                },
            ))
        }
        _ => NativePipelineOpMatch::NotThisFamily,
    })
}
