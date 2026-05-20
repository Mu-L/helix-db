use helix_dsl::prelude::*;

#[register]
fn query1(params: String) -> helix_dsl::ReadBatch {
    // helix_dsl query that returns read query or write query
    read_batch()
        .var_as(
            "user",
            g().n_where(SourcePredicate::eq("username", "alice")),
        )
        .var_as(
            "friends",
            g().n(NodeRef::var("user"))
                .out(Some("FOLLOWS"))
                .dedup()
                .limit(100),
        )
        .returning(["user", "friends"])
}

fn main() {
    let _ = helix_dsl::generate().expect("should work");
    let query = query1("alice");
}
