use crate::helix_engine::types::GraphError;
use crate::utils::items::{Edge, Node};
use heed3::{RoTxn, RwTxn};

pub trait DBMethods {
    /// Creates a new database with a given name for a secondary index
    fn create_secondary_index(&mut self, name: &str) -> Result<(), GraphError>;

    /// Opens a database with a given name for a secondary index
    fn drop_secondary_index(&mut self, name: &str) -> Result<(), GraphError>;
}

pub trait BasicStorageMethods {
    /// Gets a node object for a given node id without copying its underlying data.
    ///
    /// This should only used when fetched data is only needed temporarily
    /// as underlying data is pinned.
    fn get_temp_node<'a>(&self, txn: &'a RoTxn, id: &u128) -> Result<&'a [u8], GraphError>;

    /// Gets a edge object for a given edge id without copying its underlying data.
    ///
    /// This should only used when fetched data is only needed temporarily
    /// as underlying data is pinned.
    fn get_temp_edge<'a>(&self, txn: &'a RoTxn, id: &u128) -> Result<&'a [u8], GraphError>;
}
pub trait StorageMethods {
    /// Checks whether an entry with a given id exists.
    /// Works for nodes or edges.
    #[deprecated(note = "use get_node or get_edge instead")]
    fn check_exists(&self, txn: &RoTxn, id: &u128) -> Result<bool, GraphError>;

    /// Gets a node object for a given node id
    fn get_node<'arena>(&self, txn: &RoTxn, id: &u128, arena: &'arena bumpalo::Bump) -> Result<Node<'arena>, GraphError>;
    /// Gets a edge object for a given edge id
    fn get_edge<'arena>(&self, txn: &RoTxn, id: &u128, arena: &'arena bumpalo::Bump) -> Result<Edge<'arena>, GraphError>;

    fn drop_node(&self, txn: &mut RwTxn, id: &u128) -> Result<(), GraphError>;
    fn drop_edge(&self, txn: &mut RwTxn, id: &u128) -> Result<(), GraphError>;
    fn drop_vector(&self, txn: &mut RwTxn, id: &u128) -> Result<(), GraphError>;
}

pub trait SearchMethods {
    /// Find shortest path between two nodes
    fn shortest_path(
        &self,
        txn: &RoTxn<'_>,
        edge_label: &str,
        from_id: &u128,
        to_id: &u128,
    ) -> Result<(Vec<Node>, Vec<Edge>), GraphError>;

    fn shortest_mutual_path(
        &self,
        txn: &RoTxn<'_>,
        edge_label: &str,
        from_id: &u128,
        to_id: &u128,
    ) -> Result<(Vec<Node>, Vec<Edge>), GraphError>;
}
