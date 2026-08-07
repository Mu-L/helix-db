//! Stable storage views owned by one executable read request.
//!
//! Reader nodes pin one SlateDB [`DbSnapshot`] for the complete request.
//! Writer nodes use one ordinary [`DbTransaction`] opened at
//! [`IsolationLevel::Snapshot`]. The two variants make it impossible to build
//! a request view with the wrong storage capability or isolation level.

use std::sync::Arc;

use slatedb::{DbReadOps, DbSnapshot, DbTransaction, IsolationLevel};

use super::*;
use crate::HelixStorage;

/// One read-only request view whose storage source cannot change between steps.
pub(in crate::execution::interpreter) enum StableRequestReadView {
    /// A reader-node view pinned to one checkpoint generation and sequence.
    ReaderSnapshot(Arc<DbSnapshot>),
    /// A writer-node read transaction pinned at snapshot isolation.
    WriterTransaction(DbTransaction),
}

impl StableRequestReadView {
    /// Returns the sequence visible to this request-scoped storage view.
    pub(in crate::execution::interpreter) fn comparable_sequence(&self) -> Option<u64> {
        Some(match self {
            Self::ReaderSnapshot(snapshot) => snapshot.seq(),
            Self::WriterTransaction(transaction) => transaction.seqnum(),
        })
    }

    /// Returns whether this view must execute plan stages serially.
    ///
    /// SlateDB transactions are owned, stateful values and are never shared
    /// between concurrently executing plan steps. Reader snapshots are
    /// immutable and may be cloned into isolated parallel contexts.
    pub(in crate::execution::interpreter) const fn requires_serial_stages(&self) -> bool {
        matches!(self, Self::WriterTransaction(_))
    }

    /// Clones the immutable reader view into an isolated parallel context.
    fn clone_reader_snapshot(&self) -> Self {
        let Self::ReaderSnapshot(snapshot) = self else {
            unreachable!("writer transactions are scheduled serially")
        };
        Self::ReaderSnapshot(Arc::clone(snapshot))
    }

    /// Ends the request view and explicitly unregisters writer read transactions.
    fn close(self) {
        match self {
            Self::ReaderSnapshot(snapshot) => drop(snapshot),
            Self::WriterTransaction(transaction) => transaction.rollback(),
        }
    }
}

#[async_trait::async_trait]
impl DbReadOps for StableRequestReadView {
    async fn get_with_options<K: AsRef<[u8]> + Send>(
        &self,
        key: K,
        options: &slatedb::config::ReadOptions,
    ) -> std::result::Result<Option<bytes::Bytes>, slatedb::Error> {
        match self {
            Self::ReaderSnapshot(snapshot) => snapshot.get_with_options(key, options).await,
            Self::WriterTransaction(transaction) => {
                transaction.get_with_options(key, options).await
            }
        }
    }

    async fn get_key_value_with_options<K: AsRef<[u8]> + Send>(
        &self,
        key: K,
        options: &slatedb::config::ReadOptions,
    ) -> std::result::Result<Option<slatedb::KeyValue>, slatedb::Error> {
        match self {
            Self::ReaderSnapshot(snapshot) => {
                snapshot.get_key_value_with_options(key, options).await
            }
            Self::WriterTransaction(transaction) => {
                transaction.get_key_value_with_options(key, options).await
            }
        }
    }

    async fn multi_get_with_options<K>(
        &self,
        keys: &[K],
        options: &slatedb::config::ReadOptions,
    ) -> std::result::Result<Vec<Option<bytes::Bytes>>, slatedb::Error>
    where
        K: AsRef<[u8]> + Send + Sync,
    {
        match self {
            Self::ReaderSnapshot(snapshot) => snapshot.multi_get_with_options(keys, options).await,
            Self::WriterTransaction(transaction) => {
                transaction.multi_get_with_options(keys, options).await
            }
        }
    }

    async fn scan_with_options<T>(
        &self,
        range: T,
        options: &slatedb::config::ScanOptions,
    ) -> std::result::Result<slatedb::DbIterator, slatedb::Error>
    where
        T: slatedb::ByteRangeBounds + Send,
    {
        match self {
            Self::ReaderSnapshot(snapshot) => snapshot.scan_with_options(range, options).await,
            Self::WriterTransaction(transaction) => {
                transaction.scan_with_options(range, options).await
            }
        }
    }

    async fn scan_prefix_with_options<P, T>(
        &self,
        prefix: P,
        subrange: T,
        options: &slatedb::config::ScanOptions,
    ) -> std::result::Result<slatedb::DbIterator, slatedb::Error>
    where
        P: AsRef<[u8]> + Send,
        T: slatedb::ByteRangeBounds + Send,
    {
        match self {
            Self::ReaderSnapshot(snapshot) => {
                snapshot
                    .scan_prefix_with_options(prefix, subrange, options)
                    .await
            }
            Self::WriterTransaction(transaction) => {
                transaction
                    .scan_prefix_with_options(prefix, subrange, options)
                    .await
            }
        }
    }
}

impl<'db> ExecutionContext<'db> {
    /// Acquires the one stable storage view used by every read-plan step.
    pub(in crate::execution::interpreter) async fn enable_request_read_view(
        &mut self,
    ) -> Result<()> {
        assert!(
            self.request_read_view.is_none(),
            "request read view must be acquired exactly once"
        );
        self.request_read_view = Some(Box::new(match self.db.storage() {
            HelixStorage::Reader(reader) => {
                StableRequestReadView::ReaderSnapshot(reader.snapshot().await?)
            }
            HelixStorage::Writer(writer) => StableRequestReadView::WriterTransaction(
                writer.db().begin(IsolationLevel::Snapshot).await?,
            ),
        }));
        Ok(())
    }

    /// Verifies that a read plan acquired its request-scoped storage view.
    pub(in crate::execution::interpreter) fn validate_request_read_view(&self) -> Result<()> {
        let Some(_) = self.request_read_view.as_deref() else {
            return Err(HelixDbError::InvariantViolation(
                "read plan completed without a request read view".to_string(),
            ));
        };
        Ok(())
    }

    /// Ends a successful read view after result materialization.
    pub(in crate::execution::interpreter) fn close_request_read_view(&mut self) -> Result<()> {
        let Some(view) = self.request_read_view.take() else {
            return Err(HelixDbError::InvariantViolation(
                "read plan completed without a request read view".to_string(),
            ));
        };
        (*view).close();
        Ok(())
    }

    /// Borrows the request snapshot for storage and search operations.
    pub(in crate::execution::interpreter) fn request_read_view(
        &self,
    ) -> Option<&StableRequestReadView> {
        self.request_read_view.as_deref()
    }

    /// Returns whether this request view must keep plan stages serial.
    pub(in crate::execution::interpreter) fn request_read_view_requires_serial_stages(
        &self,
    ) -> bool {
        self.request_read_view()
            .is_some_and(StableRequestReadView::requires_serial_stages)
    }

    /// Clones a reader snapshot into one isolated parallel step context.
    pub(in crate::execution::interpreter) fn clone_parallel_request_read_view(
        &self,
    ) -> Option<Box<StableRequestReadView>> {
        self.request_read_view()
            .map(StableRequestReadView::clone_reader_snapshot)
            .map(Box::new)
    }
}

#[cfg(test)]
mod tests {
    use slatedb::object_store::memory::InMemory;

    use super::super::test_support;
    use super::*;

    #[tokio::test]
    async fn context_enforces_read_view_acquire_validate_and_close_lifecycle() {
        let db = test_support::open_db("read-view-lifecycle").await;
        let mut context = ExecutionContext::new(&db, context::ParamBindings::default());

        assert!(matches!(
            context.validate_request_read_view(),
            Err(HelixDbError::InvariantViolation(_))
        ));
        assert!(matches!(
            context.close_request_read_view(),
            Err(HelixDbError::InvariantViolation(_))
        ));

        context
            .enable_request_read_view()
            .await
            .expect("request view opens");
        context
            .validate_request_read_view()
            .expect("request view validates");
        context
            .close_request_read_view()
            .expect("request view closes");
        assert!(context.request_read_view().is_none());
    }

    #[tokio::test]
    async fn writer_transaction_keeps_its_initial_snapshot_and_requires_serial_stages() {
        let object_store: Arc<dyn slatedb::object_store::ObjectStore> = Arc::new(InMemory::new());
        let db = slatedb::Db::open("writer-read-view", object_store)
            .await
            .expect("writer opens");
        db.put(b"key", b"before").await.expect("initial value");
        let view = StableRequestReadView::WriterTransaction(
            db.begin(IsolationLevel::Snapshot)
                .await
                .expect("snapshot transaction starts"),
        );

        db.put(b"key", b"after").await.expect("concurrent value");

        assert!(view.requires_serial_stages());
        assert_eq!(
            view.get(b"key").await.expect("transaction read"),
            Some(bytes::Bytes::from_static(b"before"))
        );
        let key_value = view
            .get_key_value(b"key")
            .await
            .expect("transaction key-value read")
            .expect("transaction key-value exists");
        assert_eq!(key_value.key, bytes::Bytes::from_static(b"key"));
        assert_eq!(key_value.value, bytes::Bytes::from_static(b"before"));
        view.close();
        db.close().await.expect("writer closes");
    }

    #[tokio::test]
    async fn reader_snapshot_preserves_order_duplicates_and_missing_values() {
        let object_store: Arc<dyn slatedb::object_store::ObjectStore> = Arc::new(InMemory::new());
        let db = slatedb::Db::open("reader-read-view", Arc::clone(&object_store))
            .await
            .expect("writer opens");
        db.put(b"first", b"one").await.expect("first value");
        db.put(b"second", b"two").await.expect("second value");
        db.flush().await.expect("values flush");
        let reader = slatedb::DbReader::open(
            "reader-read-view",
            object_store,
            None,
            slatedb::config::DbReaderOptions {
                manifest_poll_interval: std::time::Duration::from_millis(10),
                ..slatedb::config::DbReaderOptions::default()
            },
        )
        .await
        .expect("reader opens");
        let view = StableRequestReadView::ReaderSnapshot(
            reader.snapshot().await.expect("reader snapshot opens"),
        );

        db.put(b"second", b"updated")
            .await
            .expect("concurrent update");
        db.put(b"second-new", b"phantom")
            .await
            .expect("concurrent prefix phantom");
        db.put(b"third", b"phantom")
            .await
            .expect("concurrent range phantom");
        db.delete(b"first").await.expect("concurrent delete");
        db.flush().await.expect("concurrent update flushes");
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if reader.get(b"second").await.expect("live reader reads")
                    == Some(bytes::Bytes::from_static(b"updated"))
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("live reader advances");

        assert!(!view.requires_serial_stages());
        assert_eq!(
            view.get(b"second").await.expect("snapshot point read"),
            Some(bytes::Bytes::from_static(b"two"))
        );
        let key_value = view
            .get_key_value(b"second")
            .await
            .expect("snapshot key-value read")
            .expect("snapshot key-value exists");
        assert_eq!(key_value.key, bytes::Bytes::from_static(b"second"));
        assert_eq!(key_value.value, bytes::Bytes::from_static(b"two"));
        assert_eq!(
            view.multi_get(&[b"second".as_slice(), b"missing", b"first", b"second"])
                .await
                .expect("snapshot multi-get"),
            vec![
                Some(bytes::Bytes::from_static(b"two")),
                None,
                Some(bytes::Bytes::from_static(b"one")),
                Some(bytes::Bytes::from_static(b"two")),
            ]
        );
        let mut range = view.scan(..).await.expect("snapshot range opens");
        let mut range_rows = Vec::new();
        while let Some(row) = range.next().await.expect("snapshot range advances") {
            range_rows.push((row.key, row.value));
        }
        assert_eq!(
            range_rows,
            vec![
                (
                    bytes::Bytes::from_static(b"first"),
                    bytes::Bytes::from_static(b"one"),
                ),
                (
                    bytes::Bytes::from_static(b"second"),
                    bytes::Bytes::from_static(b"two"),
                ),
            ]
        );
        let mut prefix = view
            .scan_prefix(b"second", ..)
            .await
            .expect("snapshot prefix opens");
        let mut prefix_rows = Vec::new();
        while let Some(row) = prefix.next().await.expect("snapshot prefix advances") {
            prefix_rows.push((row.key, row.value));
        }
        assert_eq!(
            prefix_rows,
            vec![(
                bytes::Bytes::from_static(b"second"),
                bytes::Bytes::from_static(b"two"),
            )]
        );
        let clone = view.clone_reader_snapshot();
        assert_eq!(clone.comparable_sequence(), view.comparable_sequence());

        drop((view, clone));
        reader.close().await.expect("reader closes");
        db.close().await.expect("writer closes");
    }
}
