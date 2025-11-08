QUERY SearchRecentDocuments (vector: [F64], limit: I64, cutoff_date: Date) =>
    documents <- SearchV<Document>(vector, limit)::WHERE(_::{created_at}::GTE(cutoff_date))
    RETURN documents

QUERY InsertVector (vector: [F64], content: String, created_at: Date) =>
    document <- AddV<Document>(vector, { content: content, created_at: created_at })
    doc <- document::{content, created_at}
    RETURN document

V::CogneeVector {
    collection_name: String,
    data_point_id: String,
    payload: String, // json.dumps(DataPoint) eg. (id as string, created_at, updated_at, ontology_valid, version, topological_rank, type)
    content: String,
    created_at: Date DEFAULT NOW,
    updated_at: Date DEFAULT NOW,
}