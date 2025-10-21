use heed3::PutFlags;

use crate::{
    helix_engine::{
        storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
        traversal_core::{traversal_iter::RwTraversalIterator, traversal_value::TraversalValue},
        types::GraphError,
    },
    protocol::value::Value,
    utils::properties::ImmutablePropertiesMap,
};

pub struct Update<I> {
    iter: I,
}

impl<'arena, I> Iterator for Update<I>
where
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
{
    type Item = Result<TraversalValue<'arena>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

pub trait UpdateAdapter<'db, 'arena, 'txn>: Iterator {
    fn update(
        self,
        props: Option<&[(&'static str, Value)]>,
    ) -> RwTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    UpdateAdapter<'db, 'arena, 'txn> for RwTraversalIterator<'db, 'arena, 'txn, I>
{
    fn update(
        self,
        props: Option<&[(&'static str, Value)]>,
    ) -> RwTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    > {
        // let storage = self.storage;

        // TODO: use a non-contiguous arena vec to avoid copying stuff
        // around when we run out of capacity
        let mut results = bumpalo::collections::Vec::new_in(self.arena);

        for item in self.inner {
            match item {
                Ok(value) => match value {
                    TraversalValue::Node(node) => {
                        match (props, node.properties) {
                            (Some(new), Some(old)) => {
                                // delete secondary indexes for the props changed
                                for (k, _) in new.iter() {
                                    let Some(db) = self.storage.secondary_indices.get(*k) else {
                                        continue;
                                    };

                                    let Some(old_value) = old.get(k) else {
                                        continue;
                                    };

                                    match bincode::serialize(old_value) {
                                        Ok(old_serialized) => {
                                            let Err(e) = db.delete_one_duplicate(
                                                self.txn,
                                                &old_serialized,
                                                &node.id,
                                            ) else {
                                                continue;
                                            };
                                            results.push(Err(GraphError::from(e)));
                                        }
                                        Err(e) => results.push(Err(GraphError::from(e))),
                                    }
                                }

                                // make new props, updated by current props
                                // let new_map = ImmutablePropertiesMap::new(old.len(), old.iter().map)

                                // insert new secondary indexes
                            }
                        }
                    }
                    TraversalValue::Edge(edge) => todo!(),
                    TraversalValue::Vector(hvector) => todo!(),
                    TraversalValue::VectorNodeWithoutVectorData(vector_without_data) => todo!(),
                    _ => results.push(Err(GraphError::New("Unsupported value type".to_string()))),
                },
                Err(e) => results.push(Err(e)),
            }
        }

        RwTraversalIterator {
            inner: Update {
                iter: results.into_iter(),
            },
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
        }

        // let mut vec = match self.inner.size_hint() {
        //     (_, Some(upper)) => Vec::with_capacity(upper),
        //     // no upper bound means infinite size
        //     // don't want to allocate usize::MAX sized vector
        //     _ => Vec::new(), // default vector capacity
        // };

        // for item in self.inner {
        //     match item {
        //         Ok(TraversalValue::Node(node)) => match storage.get_node(self.txn, &node.id) {
        //             Ok(mut old_node) => {
        //                 let mut properties = old_node.properties.unwrap_or_default();

        //                 if let Some(ref props) = props {
        //                     for (key, _new_value) in props.iter() {
        //                         if let Some(db) = storage.secondary_indices.get(key)
        //                             && let Some(old_value) = properties.get(key)
        //                         {
        //                             match bincode::serialize(old_value) {
        //                                 Ok(old_serialized) => {
        //                                     if let Err(e) = db.delete_one_duplicate(
        //                                         self.txn,
        //                                         &old_serialized,
        //                                         &node.id,
        //                                     ) {
        //                                         vec.push(Err(GraphError::from(e)));
        //                                     }
        //                                 }
        //                                 Err(e) => vec.push(Err(GraphError::from(e))),
        //                             }
        //                         }
        //                     }
        //                 }

        //                 if let Some(ref props) = props {
        //                     for (k, v) in props.iter() {
        //                         properties.insert(k.clone(), v.clone());
        //                     }
        //                 }

        //                 if let Some(ref props) = props {
        //                     for (key, new_value) in props.iter() {
        //                         if let Some(db) = storage.secondary_indices.get(key) {
        //                             match bincode::serialize(new_value) {
        //                                 Ok(new_serialized) => {
        //                                     if let Err(e) = db.put_with_flags(
        //                                         self.txn,
        //                                         PutFlags::APPEND_DUP,
        //                                         &new_serialized,
        //                                         &node.id,
        //                                     ) {
        //                                         vec.push(Err(GraphError::from(e)));
        //                                     }
        //                                 }
        //                                 Err(e) => vec.push(Err(GraphError::from(e))),
        //                             }
        //                         }
        //                     }
        //                 }

        //                 if properties.is_empty() {
        //                     old_node.properties = None;
        //                 } else {
        //                     old_node.properties = Some(properties);
        //                 }

        //                 match old_node.encode_node() {
        //                     Ok(serialized) => {
        //                         match storage.nodes_db.put(
        //                             self.txn,
        //                             HelixGraphStorage::node_key(&node.id),
        //                             &serialized,
        //                         ) {
        //                             Ok(_) => vec.push(Ok(TraversalValue::Node(old_node))),
        //                             Err(e) => vec.push(Err(GraphError::from(e))),
        //                         }
        //                     }
        //                     Err(e) => vec.push(Err(e)),
        //                 }
        //             }
        //             Err(e) => vec.push(Err(e)),
        //         },
        //         Ok(TraversalValue::Edge(edge)) => match storage.get_edge(self.txn, &edge.id) {
        //             Ok(old_edge) => {
        //                 let mut old_edge = old_edge.clone();
        //                 if let Some(mut properties) = old_edge.properties.clone()
        //                     && let Some(ref props) = props
        //                 {
        //                     for (k, v) in props.iter() {
        //                         properties.insert(k.clone(), v.clone());
        //                     }
        //                     old_edge.properties = Some(properties);
        //                 }
        //                 match old_edge.encode_edge() {
        //                     Ok(serialized) => {
        //                         match storage.nodes_db.put(
        //                             self.txn,
        //                             HelixGraphStorage::edge_key(&edge.id),
        //                             &serialized,
        //                         ) {
        //                             Ok(_) => vec.push(Ok(TraversalValue::Edge(old_edge))),
        //                             Err(e) => vec.push(Err(GraphError::from(e))),
        //                         }
        //                     }
        //                     Err(e) => vec.push(Err(e)),
        //                 }
        //             }
        //             Err(e) => vec.push(Err(e)),
        //         },
        //         _ => vec.push(Err(GraphError::New("Unsupported value type".to_string()))),
        //     }
        // }
        // RwTraversalIterator {
        //     inner: Update {
        //         iter: vec.into_iter(),
        //     },
        //     storage: self.storage,
        //     txn: self.txn,
        // }
    }
}
