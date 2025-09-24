QUERY find_relevant_callees(function_id: ID, query_text: String, k: I64) =>
    callees <- N(function_id)::Out
    similar_chunks <- SearchV(Embed(query_text), k)
    similar_functions <- similar_chunks::In
    relevant_callees <- callees::WHERE(
        EXISTS(similar_functions::WHERE(_::ID::EQ(callees::ID)))
    )
    RETURN relevant_callees::{id, name}