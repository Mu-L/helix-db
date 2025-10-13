use core::fmt;
use std::fmt::Display;

use crate::helixc::generator::traversal_steps::Traversal;

use super::utils::GeneratedValue;

#[derive(Clone)]
pub enum BoolOp {
    Gt(Gt),
    Gte(Gte),
    Lt(Lt),
    Lte(Lte),
    Eq(Eq),
    Neq(Neq),
    Contains(Contains),
    IsIn(IsIn),
}
impl Display for BoolOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BoolOp::Gt(gt) => format!("*v{gt}"),
            BoolOp::Gte(gte) => format!("*v{gte}"),
            BoolOp::Lt(lt) => format!("*v{lt}"),
            BoolOp::Lte(lte) => format!("*v{lte}"),
            BoolOp::Eq(eq) => format!("*v{eq}"),
            BoolOp::Neq(neq) => format!("*v{neq}"),
            BoolOp::Contains(contains) => format!("v{contains}"),
            BoolOp::IsIn(is_in) => format!("v{is_in}"),
        };
        write!(f, "map_value_or(false, |v| {s})?")
    }
}
#[derive(Clone)]
pub struct Gt {
    pub value: GeneratedValue,
}
impl Display for Gt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " > {}", self.value)
    }
}

#[derive(Clone)]
pub struct Gte {
    pub value: GeneratedValue,
}
impl Display for Gte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " >= {}", self.value)
    }
}

#[derive(Clone)]
pub struct Lt {
    pub value: GeneratedValue,
}
impl Display for Lt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " < {}", self.value)
    }
}

#[derive(Clone)]
pub struct Lte {
    pub value: GeneratedValue,
}
impl Display for Lte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " <= {}", self.value)
    }
}

#[derive(Clone)]
pub struct Eq {
    pub value: GeneratedValue,
}
impl Display for Eq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " == {}", self.value)
    }
}

#[derive(Clone)]
pub struct Neq {
    pub value: GeneratedValue,
}
impl Display for Neq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " != {}", self.value)
    }
}

#[derive(Clone)]
pub struct Contains {
    pub value: GeneratedValue,
}
impl Display for Contains {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".contains({})", self.value)
    }
}

#[derive(Clone)]
pub struct IsIn {
    pub value: GeneratedValue,
}
impl Display for IsIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".is_in({})", self.value)
    }
}

/// Boolean expression is used for a traversal or set of traversals wrapped in AND/OR
/// that resolve to a boolean value
#[derive(Clone)]
pub enum BoExp {
    Not(Box<BoExp>),
    And(Vec<BoExp>),
    Or(Vec<BoExp>),
    Exists(Traversal),
    Expr(Traversal),
    Empty,
}

impl BoExp {
    pub fn negate(&self) -> Self {
        match self {
            BoExp::Not(expr) => *expr.clone(),
            _ => BoExp::Not(Box::new(self.clone())),
        }
    }

    pub fn is_not(&self) -> bool {
        matches!(self, BoExp::Not(_))
    }
}
impl Display for BoExp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoExp::Not(expr) => write!(f, "!({expr})"),
            BoExp::And(exprs) => {
                let displayed_exprs = exprs.iter().map(|s| format!("{s}")).collect::<Vec<_>>();
                write!(f, "({})", displayed_exprs.join(" && "))
            }
            BoExp::Or(exprs) => {
                let displayed_exprs = exprs.iter().map(|s| format!("{s}")).collect::<Vec<_>>();
                write!(f, "({})", displayed_exprs.join(" || "))
            }
            BoExp::Exists(traversal) => write!(f, "Exist::exists(&mut {traversal})"),
            BoExp::Expr(traversal) => write!(f, "{traversal}"),
            BoExp::Empty => write!(f, ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helixc::generator::utils::GenRef;

    // ============================================================================
    // Comparison Operator Tests
    // ============================================================================

    #[test]
    fn test_gt_display() {
        let gt = Gt {
            value: GeneratedValue::Primitive(GenRef::Std("10".to_string())),
        };
        assert_eq!(format!("{}", gt), " > 10");
    }

    #[test]
    fn test_gte_display() {
        let gte = Gte {
            value: GeneratedValue::Primitive(GenRef::Std("5".to_string())),
        };
        assert_eq!(format!("{}", gte), " >= 5");
    }

    #[test]
    fn test_lt_display() {
        let lt = Lt {
            value: GeneratedValue::Primitive(GenRef::Std("100".to_string())),
        };
        assert_eq!(format!("{}", lt), " < 100");
    }

    #[test]
    fn test_lte_display() {
        let lte = Lte {
            value: GeneratedValue::Primitive(GenRef::Std("50".to_string())),
        };
        assert_eq!(format!("{}", lte), " <= 50");
    }

    #[test]
    fn test_eq_display() {
        let eq = Eq {
            value: GeneratedValue::Literal(GenRef::Literal("test".to_string())),
        };
        assert_eq!(format!("{}", eq), " == \"test\"");
    }

    #[test]
    fn test_neq_display() {
        let neq = Neq {
            value: GeneratedValue::Primitive(GenRef::Std("null".to_string())),
        };
        assert_eq!(format!("{}", neq), " != null");
    }

    #[test]
    fn test_contains_display() {
        let contains = Contains {
            value: GeneratedValue::Literal(GenRef::Literal("substring".to_string())),
        };
        assert_eq!(format!("{}", contains), ".contains(\"substring\")");
    }

    #[test]
    fn test_is_in_display() {
        let is_in = IsIn {
            value: GeneratedValue::Array(GenRef::Std("1, 2, 3".to_string())),
        };
        assert_eq!(format!("{}", is_in), ".is_in(&[1, 2, 3])");
    }

    // ============================================================================
    // BoolOp Tests
    // ============================================================================

    #[test]
    fn test_boolop_gt_wrapped() {
        let bool_op = BoolOp::Gt(Gt {
            value: GeneratedValue::Primitive(GenRef::Std("20".to_string())),
        });
        let output = format!("{}", bool_op);
        assert!(output.contains("map_value_or(false, |v| *v > 20)"));
    }

    #[test]
    fn test_boolop_eq_wrapped() {
        let bool_op = BoolOp::Eq(Eq {
            value: GeneratedValue::Literal(GenRef::Literal("value".to_string())),
        });
        let output = format!("{}", bool_op);
        assert!(output.contains("map_value_or(false, |v| *v == \"value\")"));
    }

    #[test]
    fn test_boolop_contains_wrapped() {
        let bool_op = BoolOp::Contains(Contains {
            value: GeneratedValue::Literal(GenRef::Literal("text".to_string())),
        });
        let output = format!("{}", bool_op);
        assert!(output.contains("map_value_or(false, |v| v.contains(\"text\"))"));
    }

    // ============================================================================
    // BoExp Tests
    // ============================================================================

    #[test]
    fn test_boexp_empty() {
        let boexp = BoExp::Empty;
        assert_eq!(format!("{}", boexp), "");
    }

    #[test]
    fn test_boexp_negate() {
        let boexp = BoExp::Empty;
        let negated = boexp.negate();
        assert!(negated.is_not());
    }

    #[test]
    fn test_boexp_double_negate() {
        let boexp = BoExp::Empty;
        let negated = boexp.negate();
        let double_negated = negated.negate();
        assert!(!double_negated.is_not());
    }

    #[test]
    fn test_boexp_is_not() {
        let not_expr = BoExp::Not(Box::new(BoExp::Empty));
        assert!(not_expr.is_not());

        let normal_expr = BoExp::Empty;
        assert!(!normal_expr.is_not());
    }
}
