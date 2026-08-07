//! Fuzzes public planner-to-interpreter execution on an empty graph model.

#![no_main]

use std::sync::LazyLock;

use db::{HelixDB, HelixDbSource};
use helix_ast::batch;
use helix_ast::graph::NodeRef;
use helix_ast::query::QueryRequest;
use helix_ast::traversal;
use libfuzzer_sys::fuzz_target;

static RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("planner fuzz runtime must initialize"));

static DATABASE: LazyLock<HelixDB> = LazyLock::new(|| {
    RUNTIME
        .block_on(HelixDB::open(HelixDbSource::InMemory {
            database: "planner-interpreter-fuzz".to_string(),
        }))
        .expect("planner fuzz database must open")
});

fuzz_target!(|data: &[u8]| {
    let Some(selector) = data.first() else {
        return;
    };
    let traversal = match selector % 5 {
        0 => traversal::g().n(NodeRef::all()).count(),
        1 => traversal::g().n(NodeRef::id(u64::from(*selector))).count(),
        2 => traversal::g().n_with_label("FuzzNode").count(),
        3 => traversal::g()
            .n(NodeRef::id(u64::from(*selector)))
            .out(None::<&str>)
            .count(),
        _ => traversal::g()
            .n(NodeRef::all())
            .range(usize::from(*selector % 4), usize::from(*selector % 4) + 1)
            .count(),
    };
    let request = QueryRequest::read(
        batch::read_batch()
            .var_as("result", traversal)
            .returning(["result"]),
    );
    let result = RUNTIME
        .block_on(DATABASE.query(request))
        .expect("valid empty-graph query must execute");
    assert_eq!(result["result"], 0);
});
