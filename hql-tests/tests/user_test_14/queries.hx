V::DocumentChunk {
    firmId: String
}

QUERY SearchChunksVectorByFirm(queryVector: [F64], firmId: String, limit: I64) =>
results <- SearchV<DocumentChunk>(queryVector, limit)::RerankRRF(k: 60)
::WHERE(_::{firmId}::EQ(firmId))
    RETURN results
