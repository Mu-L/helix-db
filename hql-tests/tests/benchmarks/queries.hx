// HelixQL Schema Definition
// Based on the provided Rust structs

// Node: UserRecord
// Note: 'id' field is implicit in HelixQL and not declared in schema
N::User {
    country: U8
}

// Vector: ItemRecord
// Note: 'id' field is implicit and 'embedding' is implicit for vectors
V::Item {
    category: U16
}

// Edge: EdgeRecord
// Connects User nodes to Item nodes
E::Interacted {
    From: User,
    To: Item
}

N::Metadata {
    INDEX key: String,
    value: String
}


QUERY PointGet(item_id: ID) =>
    item <- V<Item>(item_id)
    RETURN item::{id, category}

QUERY OneHop(user_id: ID) =>
    user <- N<User>(user_id)
    items <- user::Out<Interacted>
    RETURN items::{id, category}

QUERY OneHopFilter(user_id: ID, category: U16) =>
    user <- N<User>(user_id)
    items <- user::Out<Interacted>::WHERE(_::{category}::EQ(category))
    RETURN items::{id, category}

QUERY Vector(vector: [F64], top_k: I64) =>
    items <- SearchV<Item>(vector, top_k)
    RETURN items::{id, score, category}

QUERY VectorHopFilter(vector: [F64], top_k: I64, country: U8) =>
    similar_items <- SearchV<Item>(vector, top_k)
    items <- similar_items::WHERE(EXISTS(_::In<Interacted>::WHERE(_::{country}::EQ(country))))
    RETURN items::{id, category}
    
QUERY GetDatasetId() =>
    dataset_id <- N<Metadata>({ key: "dataset_id" })
    RETURN dataset_id::{value}