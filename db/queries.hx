N::FUNCTION {
    name: String,
    code: String,
    created_at: String,
    updated_at: String,
}

V::CODE_CHUNK {}

E::CALLS {
    From: FUNCTION,
    To: FUNCTION,
    Properties: {
        created_at: String,
    }
}

E::HAS_EMBEDDING {
    From: FUNCTION,
    To: CODE_CHUNK,
    Properties: {
        created_at: String,
    }
}

QUERY find_relevant_callees(function_id: ID, query_text: String, k: I64) =>
    callees <- N<FUNCTION>(function_id)::Out<CALLS>
    similar_chunks <- SearchV<CODE_CHUNK>(Embed(query_text), k)
    similar_functions <- similar_chunks::In<HAS_EMBEDDING>
    relevant_callees <- callees::WHERE(
        EXISTS(similar_functions::WHERE(_::ID::EQ(callees::ID)))
    )
    RETURN relevant_callees::{id, name}