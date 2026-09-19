//! Materialized boundary values and their incremental item representation.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    Rows,
    Scalars,
    Count,
    Bool,
    Folded,
}

impl Shape {
    pub(super) fn of(value: &ExecutionValue) -> Result<Self> {
        match value {
            ExecutionValue::Stream(_) => Ok(Self::Rows),
            ExecutionValue::Scalars(_) => Ok(Self::Scalars),
            ExecutionValue::Count(_) => Ok(Self::Count),
            ExecutionValue::Bool(_) => Ok(Self::Bool),
            ExecutionValue::FoldedStream(_) => Ok(Self::Folded),
            ExecutionValue::IndexDdlReceipt(_) | ExecutionValue::IndexOperationStatus(_) => Err(
                HelixDbError::Query("cannot concatenate index lifecycle dependency outputs".into()),
            ),
        }
    }

    pub(super) fn empty(self) -> ExecutionValue {
        match self {
            Self::Rows => ExecutionValue::Stream(Vec::new()),
            Self::Scalars => ExecutionValue::Scalars(Vec::new()),
            Self::Count => ExecutionValue::Count(0),
            Self::Bool => ExecutionValue::Bool(false),
            Self::Folded => ExecutionValue::FoldedStream(FoldedStream::new(Vec::new())),
        }
    }

    pub(super) fn window(self, operation: &str) -> Result<Self> {
        match self {
            Self::Rows => Ok(Self::Rows),
            Self::Scalars | Self::Count | Self::Bool => Ok(Self::Scalars),
            Self::Folded => Err(HelixDbError::Query(format!(
                "{operation} expected stream input, got folded stream; use unfold first"
            ))),
        }
    }
}

pub(super) enum Items {
    Rows(std::vec::IntoIter<ExecutionRow>),
    Scalars(std::vec::IntoIter<ExecutionScalar>),
    Terminal(Option<ExecutionValue>),
}

impl Items {
    pub(super) fn new(value: ExecutionValue) -> Self {
        match value {
            ExecutionValue::Stream(rows) => Self::Rows(rows.into_iter()),
            ExecutionValue::Scalars(values) => Self::Scalars(values.into_iter()),
            value @ ExecutionValue::FoldedStream(_)
            | value @ ExecutionValue::Count(_)
            | value @ ExecutionValue::Bool(_)
            | value @ ExecutionValue::IndexDdlReceipt(_)
            | value @ ExecutionValue::IndexOperationStatus(_) => Self::Terminal(Some(value)),
        }
    }

    pub(super) fn next(&mut self) -> Option<ExecutionValue> {
        match self {
            Self::Rows(rows) => rows.next().map(|row| ExecutionValue::Stream(vec![row])),
            Self::Scalars(values) => values
                .next()
                .map(|value| ExecutionValue::Scalars(vec![value])),
            Self::Terminal(value) => value.take(),
        }
    }
}

pub(super) fn append(out: &mut ExecutionValue, item: ExecutionValue) -> Result<()> {
    match (out, item) {
        (ExecutionValue::Stream(out), ExecutionValue::Stream(mut rows)) => out.append(&mut rows),
        (ExecutionValue::Scalars(out), ExecutionValue::Scalars(mut items)) => {
            out.append(&mut items)
        }
        (out @ ExecutionValue::Count(_), item @ ExecutionValue::Count(_))
        | (out @ ExecutionValue::Bool(_), item @ ExecutionValue::Bool(_))
        | (out @ ExecutionValue::FoldedStream(_), item @ ExecutionValue::FoldedStream(_)) => {
            *out = item
        }
        _ => {
            return Err(HelixDbError::InvariantViolation(
                "pull output changed value shape".into(),
            ))
        }
    }
    Ok(())
}
