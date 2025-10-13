use core::fmt;
use std::{collections::HashMap, fmt::Display};

use crate::helixc::generator::{traversal_steps::Traversal, utils::GeneratedValue};

pub struct ReturnValue {
    pub value: ReturnValueExpr,
    pub return_type: ReturnType,
}
impl Display for ReturnValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.return_type {
            ReturnType::Literal(name) => {
                if let GeneratedValue::Aggregate(name) = name {
                    writeln!(
                        f,
                        "    return_vals.insert({}.to_string(), ReturnValue::from({}));",
                        name, self.value
                    )
                } else {
                    writeln!(
                        f,
                        "    return_vals.insert({}.to_string(), ReturnValue::from(Value::from({})));",
                        name, self.value
                    )
                }
            }
            ReturnType::NamedLiteral(name) => {
                writeln!(
                    f,
                    "    return_vals.insert({}.to_string(), ReturnValue::from(Value::from({})));",
                    name, self.value
                )
            }
            ReturnType::NamedExpr(name) => {
                writeln!(
                    f,
                    "    return_vals.insert({}.to_string(), ReturnValue::from_traversal_value_array_with_mixin({}, remapping_vals.borrow_mut()));",
                    name, self.value
                )
            }
            ReturnType::SingleExpr(name) => {
                writeln!(
                    f,
                    "    return_vals.insert({}.to_string(), ReturnValue::from_traversal_value_with_mixin({}, remapping_vals.borrow_mut()));",
                    name, self.value
                )
            }
            ReturnType::UnnamedExpr => {
                writeln!(
                    f,
                    "    return_vals.insert(\"data\".to_string(), ReturnValue::from_traversal_value_array_with_mixin({}, remapping_vals.borrow_mut()));",
                    self.value
                )
            }
            ReturnType::HashMap => {
                writeln!(
                    f,
                    "    return_vals.insert(\"data\".to_string(), ReturnValue::from({}));",
                    self.value
                )
            }
            ReturnType::Array => {
                writeln!(
                    f,
                    "    return_vals.insert(\"data\".to_string(), ReturnValue::from({}));",
                    self.value
                )
            }
            ReturnType::Aggregate(name) => {
                writeln!(
                    f,
                    "    return_vals.insert({}.to_string(), ReturnValue::from({}));",
                    name, self.value
                )
            }
        }
    }
}

impl ReturnValue {
    pub fn get_name(&self) -> String {
        match &self.return_type {
            ReturnType::Literal(name) => name.inner().inner().to_string(),
            ReturnType::NamedLiteral(name) => name.inner().inner().to_string(),
            ReturnType::NamedExpr(name) => name.inner().inner().to_string(),
            ReturnType::SingleExpr(name) => name.inner().inner().to_string(),
            ReturnType::UnnamedExpr => unimplemented!(),
            ReturnType::HashMap => unimplemented!(),
            ReturnType::Array => unimplemented!(),
            ReturnType::Aggregate(name) => name.to_string(),
        }
    }

    pub fn new_literal(name: GeneratedValue, value: GeneratedValue) -> Self {
        Self {
            value: ReturnValueExpr::Value(value.clone()),
            return_type: ReturnType::Literal(name),
        }
    }
    pub fn new_named_literal(name: GeneratedValue, value: GeneratedValue) -> Self {
        Self {
            value: ReturnValueExpr::Value(value.clone()),
            return_type: ReturnType::NamedLiteral(name),
        }
    }
    pub fn new_named(name: GeneratedValue, value: ReturnValueExpr) -> Self {
        Self {
            value,
            return_type: ReturnType::NamedExpr(name),
        }
    }
    pub fn new_single_named(name: GeneratedValue, value: ReturnValueExpr) -> Self {
        Self {
            value,
            return_type: ReturnType::SingleExpr(name),
        }
    }
    pub fn new_unnamed(value: ReturnValueExpr) -> Self {
        Self {
            value,
            return_type: ReturnType::UnnamedExpr,
        }
    }
    pub fn new_array(values: Vec<ReturnValueExpr>) -> Self {
        Self {
            value: ReturnValueExpr::Array(values),
            return_type: ReturnType::Array,
        }
    }
    pub fn new_object(values: HashMap<String, ReturnValueExpr>) -> Self {
        Self {
            value: ReturnValueExpr::Object(values),
            return_type: ReturnType::HashMap,
        }
    }
    pub fn new_aggregate(name: GeneratedValue, value: GeneratedValue) -> Self {
        Self {
            value: ReturnValueExpr::Value(value.clone()),
            return_type: ReturnType::Aggregate(name),
        }
    }
    pub fn new_aggregate_traversal(name: GeneratedValue, value: ReturnValueExpr) -> Self {
        Self {
            value,
            return_type: ReturnType::Aggregate(name),
        }
    }
}

#[derive(Clone)]
pub enum ReturnType {
    Literal(GeneratedValue),
    NamedLiteral(GeneratedValue),
    NamedExpr(GeneratedValue),
    SingleExpr(GeneratedValue),
    UnnamedExpr,
    HashMap,
    Array,
    Aggregate(GeneratedValue),
}
#[derive(Clone)]
pub enum ReturnValueExpr {
    Traversal(Traversal),
    Identifier(GeneratedValue),
    Value(GeneratedValue),
    Array(Vec<ReturnValueExpr>),
    Object(HashMap<String, ReturnValueExpr>),
}
impl Display for ReturnValueExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReturnValueExpr::Traversal(traversal) => write!(f, "{traversal}"),
            ReturnValueExpr::Identifier(identifier) => write!(f, "{identifier}"),
            ReturnValueExpr::Value(value) => write!(f, "{value}"),
            ReturnValueExpr::Array(values) => {
                write!(f, "vec![")?;
                // if traversal then use the other from functions
                for value in values {
                    write!(f, "ReturnValue::from({value}),")?;
                }
                write!(f, "]")
            }
            ReturnValueExpr::Object(values) => {
                write!(f, "HashMap::from([")?;
                // if traversal then use the other from functions
                for (key, value) in values {
                    write!(f, "(String::from(\"{key}\"), ReturnValue::from({value})),")?;
                }
                write!(f, "])")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helixc::generator::utils::GenRef;

    // ============================================================================
    // ReturnValueExpr Tests
    // ============================================================================

    #[test]
    fn test_return_value_expr_value() {
        let expr = ReturnValueExpr::Value(GeneratedValue::Literal(GenRef::Literal(
            "test".to_string(),
        )));
        assert_eq!(format!("{}", expr), "\"test\"");
    }

    #[test]
    fn test_return_value_expr_identifier() {
        let expr = ReturnValueExpr::Identifier(GeneratedValue::Identifier(GenRef::Std(
            "var".to_string(),
        )));
        assert_eq!(format!("{}", expr), "var");
    }

    #[test]
    fn test_return_value_expr_empty_array() {
        let expr = ReturnValueExpr::Array(vec![]);
        assert_eq!(format!("{}", expr), "vec![]");
    }

    #[test]
    fn test_return_value_expr_array_with_values() {
        let expr = ReturnValueExpr::Array(vec![
            ReturnValueExpr::Value(GeneratedValue::Literal(GenRef::Literal("a".to_string()))),
            ReturnValueExpr::Value(GeneratedValue::Literal(GenRef::Literal("b".to_string()))),
        ]);
        let output = format!("{}", expr);
        assert!(output.contains("vec!["));
        assert!(output.contains("ReturnValue::from(\"a\")"));
        assert!(output.contains("ReturnValue::from(\"b\")"));
    }

    #[test]
    fn test_return_value_expr_empty_object() {
        let expr = ReturnValueExpr::Object(HashMap::new());
        assert_eq!(format!("{}", expr), "HashMap::from([])");
    }

    #[test]
    fn test_return_value_expr_object_with_values() {
        let mut map = HashMap::new();
        map.insert(
            "key1".to_string(),
            ReturnValueExpr::Value(GeneratedValue::Literal(GenRef::Literal(
                "value1".to_string(),
            ))),
        );
        let expr = ReturnValueExpr::Object(map);
        let output = format!("{}", expr);
        assert!(output.contains("HashMap::from(["));
        assert!(output.contains("String::from(\"key1\")"));
        assert!(output.contains("ReturnValue::from(\"value1\")"));
    }

    // ============================================================================
    // ReturnValue Constructor Tests
    // ============================================================================

    #[test]
    fn test_return_value_new_literal() {
        let name = GeneratedValue::Literal(GenRef::Literal("result".to_string()));
        let value = GeneratedValue::Primitive(GenRef::Std("42".to_string()));
        let ret_val = ReturnValue::new_literal(name, value);
        assert_eq!(ret_val.get_name(), "result");
    }

    #[test]
    fn test_return_value_new_named_literal() {
        let name = GeneratedValue::Literal(GenRef::Literal("count".to_string()));
        let value = GeneratedValue::Primitive(GenRef::Std("100".to_string()));
        let ret_val = ReturnValue::new_named_literal(name, value);
        assert_eq!(ret_val.get_name(), "count");
    }

    #[test]
    fn test_return_value_new_array() {
        let values = vec![
            ReturnValueExpr::Value(GeneratedValue::Primitive(GenRef::Std("1".to_string()))),
            ReturnValueExpr::Value(GeneratedValue::Primitive(GenRef::Std("2".to_string()))),
        ];
        let ret_val = ReturnValue::new_array(values);
        let output = format!("{}", ret_val);
        assert!(output.contains("\"data\""));
    }

    #[test]
    fn test_return_value_new_object() {
        let mut map = HashMap::new();
        map.insert(
            "field".to_string(),
            ReturnValueExpr::Value(GeneratedValue::Literal(GenRef::Literal(
                "value".to_string(),
            ))),
        );
        let ret_val = ReturnValue::new_object(map);
        let output = format!("{}", ret_val);
        assert!(output.contains("\"data\""));
    }

    #[test]
    fn test_return_value_new_aggregate() {
        let name = GeneratedValue::Literal(GenRef::Literal("total".to_string()));
        let value = GeneratedValue::Aggregate(GenRef::Std("sum_value".to_string()));
        let ret_val = ReturnValue::new_aggregate(name, value);
        let output = format!("{}", ret_val);
        assert!(output.contains("\"total\""));
        assert!(output.contains("sum_value"));
    }

    #[test]
    fn test_return_value_new_unnamed() {
        let value = ReturnValueExpr::Value(GeneratedValue::Identifier(GenRef::Std(
            "result".to_string(),
        )));
        let ret_val = ReturnValue::new_unnamed(value);
        let output = format!("{}", ret_val);
        assert!(output.contains("\"data\""));
    }

    #[test]
    fn test_return_value_display_literal() {
        let name = GeneratedValue::Literal(GenRef::Literal("output".to_string()));
        let value = GeneratedValue::Primitive(GenRef::Std("true".to_string()));
        let ret_val = ReturnValue::new_literal(name, value);
        let output = format!("{}", ret_val);
        assert!(output.contains("return_vals.insert"));
        assert!(output.contains("\"output\""));
    }

    #[test]
    fn test_return_value_display_named_expr() {
        let name = GeneratedValue::Literal(GenRef::Literal("users".to_string()));
        let value = ReturnValueExpr::Identifier(GeneratedValue::Identifier(GenRef::Std(
            "user_list".to_string(),
        )));
        let ret_val = ReturnValue::new_named(name, value);
        let output = format!("{}", ret_val);
        assert!(output.contains("return_vals.insert"));
        assert!(output.contains("\"users\""));
        assert!(output.contains("from_traversal_value_array_with_mixin"));
    }

    #[test]
    fn test_return_value_display_single_expr() {
        let name = GeneratedValue::Literal(GenRef::Literal("user".to_string()));
        let value = ReturnValueExpr::Identifier(GeneratedValue::Identifier(GenRef::Std(
            "single_user".to_string(),
        )));
        let ret_val = ReturnValue::new_single_named(name, value);
        let output = format!("{}", ret_val);
        assert!(output.contains("return_vals.insert"));
        assert!(output.contains("\"user\""));
        assert!(output.contains("from_traversal_value_with_mixin"));
    }
}
