//! # helix-db Rust SDK
//!
//! Crate root. The query-builder DSL lives in [`dsl`] and the query bundle /
//! code-generation support lives in [`query_generator`].
//!
//! Most application code only needs the curated builder API:
//! ```
//! use helix_dsl::prelude::*;
//! ```

mod dsl;
pub mod query_generator;

// Re-export the DSL surface (types, builders, `prelude`, etc.) at the crate
// root. This is also what makes the `crate::*` paths used inside `dsl.rs` and
// `query_generator.rs` resolve.
pub use dsl::*;

// Convenience re-export so `helix_dsl::prelude::*` is reachable directly.
pub use dsl::prelude;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct HelixDBClient {
    client: Client,
    url: reqwest::Url,
    api_key: Option<String>,
}

#[derive(Debug, Error)]
pub enum HelixError {
    #[error("Error communicating with server: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Got Error from server: {details}")]
    RemoteError { details: String },
    #[error("Error serializing data: {0}")]
    SerializationError(#[from] sonic_rs::Error),
    #[error("Invalid URL: {0}")]
    InvalidURL(String),
}

// This trait allows users to implement their own client if needed

impl HelixDBClient {
    pub fn new(url: Option<&str>, api_key: Option<&str>) -> Result<Self, HelixError> {
        let url = reqwest::Url::parse(url.unwrap_or("http://localhost:6969"))
            .map_err(|e| HelixError::InvalidURL(e.to_string()))?;
        Ok(Self {
            client: Client::new(),
            url,
            api_key: api_key.map(|key| key.to_string()),
        })
    }

    pub async fn dynamic_query<R>(&self, query: &DynamicQueryRequest) -> Result<R, HelixError>
    where
        R: for<'de> Deserialize<'de>,
    {
        let mut request = self
            .client
            .post(self.url.join("/v1/query").unwrap())
            .header("Content-Type", "application/json")
            .body(sonic_rs::to_vec(query)?);

        // Add API key header if provided
        if let Some(ref api_key) = self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request.send().await?;

        match response.status() {
            StatusCode::OK => {
                let bytes = response.bytes().await?;
                sonic_rs::from_slice::<R>(&bytes).map_err(Into::into)
            }
            code => match response.text().await {
                Ok(t) => Err(HelixError::RemoteError { details: t }),
                Err(_) => match code.canonical_reason() {
                    Some(r) => Err(HelixError::RemoteError {
                        details: r.to_string(),
                    }),
                    None => Err(HelixError::RemoteError {
                        details: format!("unkown error with code: {code}"),
                    }),
                },
            },
        }
    }

    pub async fn stored_query<T, R>(&self, query_path: &str, data: &T) -> Result<R, HelixError>
    where
        T: Serialize + Sync,
        R: for<'de> Deserialize<'de>,
    {
        let url = self
            .url
            .join(query_path)
            .map_err(|e| HelixError::InvalidURL(e.to_string()))?;
        let mut request = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(sonic_rs::to_vec(data)?);

        // Add API key header if provided
        if let Some(ref api_key) = self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request.send().await?;

        match response.status() {
            StatusCode::OK => {
                let bytes = response.bytes().await?;
                sonic_rs::from_slice::<R>(&bytes).map_err(Into::into)
            }
            code => match response.text().await {
                Ok(t) => Err(HelixError::RemoteError { details: t }),
                Err(_) => match code.canonical_reason() {
                    Some(r) => Err(HelixError::RemoteError {
                        details: r.to_string(),
                    }),
                    None => Err(HelixError::RemoteError {
                        details: format!("unkown error with code: {code}"),
                    }),
                },
            },
        }
    }
}

extern crate self as helix_dsl;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[register]
    fn query1(name: String) {
        // helix_dsl query that returns read query or write query
        read_batch()
            .var_as("user", g().n_where(SourcePredicate::eq("username", name)))
            .var_as(
                "friends",
                g().n(NodeRef::var("user"))
                    .out(Some("FOLLOWS"))
                    .dedup()
                    .limit(100),
            )
            .returning(["user", "friends"])
    }

    #[test]
    fn query1_builds_dynamic_request() {
        // Calling the registered fn with concrete args yields a DynamicQueryRequest directly.
        let query = query1(String::from("alice"));

        assert!(matches!(query.request_type, DynamicQueryRequestType::Read));
        let params = query.parameters.expect("parameters present");
        assert!(matches!(
            params.get("name"),
            Some(DynamicQueryValue::String(s)) if s == "alice"
        ));
    }

    // ---- Group 1: every #[register] param type coerces correctly -----------

    #[register]
    fn q_bool(flag: bool) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", flag)))
            .returning(["v"])
    }
    #[register]
    fn q_i64(num: i64) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", num)))
            .returning(["v"])
    }
    #[register]
    fn q_f64(x: f64) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", x)))
            .returning(["v"])
    }
    #[register]
    fn q_f32(x: f32) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", x)))
            .returning(["v"])
    }
    #[register]
    fn q_datetime(ts: DateTime) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", ts)))
            .returning(["v"])
    }
    #[register]
    fn q_value(val: ParamValue) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", val)))
            .returning(["v"])
    }
    #[register]
    fn q_object(obj: ParamObject) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", obj)))
            .returning(["v"])
    }
    #[register]
    fn q_array(items: Vec<String>) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", items)))
            .returning(["v"])
    }
    #[register]
    fn q_map(map: BTreeMap<String, String>) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", map)))
            .returning(["v"])
    }
    #[register]
    #[allow(unused_variables)] // bytes coercion errors without reading the value (see test below)
    fn q_bytes(blob: Vec<u8>) {
        read_batch()
            .var_as("v", g().n_where(SourcePredicate::eq("field", blob)))
            .returning(["v"])
    }

    #[test]
    fn param_types_coerce_correctly() {
        // bool
        let r = q_bool(true);
        assert!(matches!(r.request_type, DynamicQueryRequestType::Read));
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("flag"),
            Some(DynamicQueryValue::Bool(true))
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("flag"),
            Some(QueryParamType::Bool)
        ));

        // i64
        let r = q_i64(7);
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("num"),
            Some(DynamicQueryValue::I64(7))
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("num"),
            Some(QueryParamType::I64)
        ));

        // f64
        let r = q_f64(1.5);
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("x"),
            Some(DynamicQueryValue::F64(v)) if *v == 1.5
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("x"),
            Some(QueryParamType::F64)
        ));

        // f32
        let r = q_f32(1.5f32);
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("x"),
            Some(DynamicQueryValue::F32(v)) if *v == 1.5f32
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("x"),
            Some(QueryParamType::F32)
        ));

        // DateTime -> rfc3339 string
        let r = q_datetime(DateTime::from_millis(0));
        let expected = DateTime::from_millis(0).to_rfc3339().unwrap();
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("ts"),
            Some(DynamicQueryValue::String(s)) if *s == expected
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("ts"),
            Some(QueryParamType::DateTime)
        ));

        // ParamValue (PropertyValue)
        let r = q_value(PropertyValue::I64(5));
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("val"),
            Some(DynamicQueryValue::I64(5))
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("val"),
            Some(QueryParamType::Value)
        ));

        // ParamObject (BTreeMap<String, PropertyValue>)
        let mut obj = BTreeMap::new();
        obj.insert("k".to_string(), PropertyValue::String("x".to_string()));
        let r = q_object(obj);
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("obj"),
            Some(DynamicQueryValue::Object(_))
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("obj"),
            Some(QueryParamType::Object)
        ));

        // Vec<String> -> Array(String)
        let r = q_array(vec!["a".to_string(), "b".to_string()]);
        match r.parameters.as_ref().unwrap().get("items") {
            Some(DynamicQueryValue::Array(items)) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], DynamicQueryValue::String(s) if s == "a"));
                assert!(matches!(&items[1], DynamicQueryValue::String(s) if s == "b"));
            }
            other => panic!("expected array, got {other:?}"),
        }
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("items"),
            Some(QueryParamType::Array(inner)) if matches!(**inner, QueryParamType::String)
        ));

        // BTreeMap<String, String> -> Object
        let mut map = BTreeMap::new();
        map.insert("k".to_string(), "v".to_string());
        let r = q_map(map);
        assert!(matches!(
            r.parameters.as_ref().unwrap().get("map"),
            Some(DynamicQueryValue::Object(_))
        ));
        assert!(matches!(
            r.parameter_types.as_ref().unwrap().get("map"),
            Some(QueryParamType::Object)
        ));
    }

    #[test]
    #[should_panic(expected = "failed to coerce parameter")]
    fn bytes_param_panics_on_dynamic_call() {
        // Bytes params register fine for the stored query, but dynamic coercion is unsupported
        // and the generated callable panics when invoked.
        let _ = q_bytes(vec![1, 2, 3]);
    }

    // ---- Group 2: SourcePredicate JSON — old (literal) vs new (param) -------

    #[test]
    fn source_predicate_literal_json_is_unchanged() {
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::eq("username", "alice")).unwrap(),
            r#"{"Eq":["username",{"String":"alice"}]}"#
        );
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::gt("score", 10i64)).unwrap(),
            r#"{"Gt":["score",{"I64":10}]}"#
        );
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::between("age", 18i64, 65i64)).unwrap(),
            r#"{"Between":["age",{"I64":18},{"I64":65}]}"#
        );
    }

    #[test]
    fn source_predicate_param_json_uses_expr_variants() {
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::eq("username", Expr::param("name"))).unwrap(),
            r#"{"EqExpr":["username",{"Param":"name"}]}"#
        );
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::lte("score", Expr::param("max"))).unwrap(),
            r#"{"LteExpr":["score",{"Param":"max"}]}"#
        );
        assert_eq!(
            sonic_rs::to_string(&SourcePredicate::between("age", Expr::param("lo"), 65i64))
                .unwrap(),
            r#"{"BetweenExpr":["age",{"Param":"lo"},{"Constant":{"I64":65}}]}"#
        );
    }

    #[test]
    fn source_predicate_json_round_trips() {
        for sp in [
            SourcePredicate::eq("username", "alice"),
            SourcePredicate::eq("username", Expr::param("name")),
            SourcePredicate::between("age", Expr::param("lo"), 65i64),
        ] {
            let json = sonic_rs::to_string(&sp).unwrap();
            let back: SourcePredicate = sonic_rs::from_str(&json).unwrap();
            assert_eq!(sp, back);
        }
    }

    // ---- Group 3: full query AST, literal vs param (self-contained) --------

    #[test]
    fn query_ast_literal_vs_param_json() {
        let literal = read_batch()
            .var_as(
                "user",
                g().n_where(SourcePredicate::eq("username", "alice")),
            )
            .returning(["user"]);
        let literal_json = sonic_rs::to_string(&literal).unwrap();
        assert!(
            literal_json.contains(r#"{"NWhere":{"Eq":["username",{"String":"alice"}]}}"#),
            "literal NWhere step changed shape: {literal_json}"
        );
        assert!(!literal_json.contains("EqExpr"));

        let param = read_batch()
            .var_as(
                "user",
                g().n_where(SourcePredicate::eq("username", Expr::param("name"))),
            )
            .returning(["user"]);
        let param_json = sonic_rs::to_string(&param).unwrap();
        assert!(
            param_json.contains(r#"{"NWhere":{"EqExpr":["username",{"Param":"name"}]}}"#),
            "param NWhere step missing EqExpr/Param: {param_json}"
        );
    }
}
