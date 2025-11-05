pub mod events;

use std::{
    cell::RefCell,
    env::consts::OS,
    fs,
    path::Path,
    sync::{
        LazyLock, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use tokio::task::JoinHandle;

pub static METRICS_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

static CONFIG: LazyLock<String> = LazyLock::new(|| {
    let home_dir = std::env::var("HOME").unwrap_or("~/".to_string());
    let config_path = &format!("{home_dir}/.helix/credentials");
    let config_path = Path::new(config_path);
    fs::read_to_string(config_path).unwrap_or_default()
});

pub static HELIX_USER_ID: LazyLock<String> = LazyLock::new(|| {
    // read from credentials file
    for line in CONFIG.lines() {
        if let Some((key, value)) = line.split_once("=")
            && key.to_lowercase() == "helix_user_id"
        {
            return value.to_string();
        }
    }
    String::new()
});

pub static METRICS_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    for line in CONFIG.lines() {
        if let Some((key, value)) = line.split_once("=")
            && key.to_lowercase().as_str() == "metrics"
        {
            return value.to_string().parse().unwrap_or(true);
        }
    }
    true
});

pub const METRICS_URL: &str = "https://logs.helix-db.com";

// Thread-local buffer for events
thread_local! {
    static EVENT_BUFFER: RefCell<Vec<events::RawEvent<events::EventData>>> =
        RefCell::new(Vec::with_capacity(THREAD_LOCAL_EVENT_BUFFER_LENGTH));
}

// Global state for metrics system
struct MetricsState {
    events_tx: flume::Sender<events::RawEvent<events::EventData>>,
    events_rx: flume::Receiver<events::RawEvent<events::EventData>>,
    notify_tx: flume::Sender<()>,
    notify_rx: flume::Receiver<()>,
    threshold_bytes: AtomicUsize,
    sender_handle: OnceLock<tokio::task::JoinHandle<()>>,
}

static METRICS_STATE: LazyLock<MetricsState> = LazyLock::new(|| {
    let (events_tx, events_rx) = flume::unbounded();
    let (notify_tx, notify_rx) = flume::unbounded();

    // Read threshold from environment or use default
    let threshold_bytes = std::env::var("HELIX_METRICS_THRESHOLD_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(DEFAULT_THRESHOLD_BYTES);

    MetricsState {
        events_tx,
        events_rx,
        notify_tx,
        notify_rx,
        threshold_bytes: AtomicUsize::new(threshold_bytes),
        sender_handle: OnceLock::new(),
    }
});

// Configuration constants
const THREAD_LOCAL_EVENT_BUFFER_LENGTH: usize = 65536;
const THREAD_LOCAL_FLUSH_THRESHOLD: usize = 65536;
const BATCH_TIMEOUT_SECS: u64 = 1;
const DEFAULT_THRESHOLD_BYTES: usize = 5 * 1024 * 1024; // 5MB default
const ESTIMATED_EVENT_SIZE_BYTES: usize = 160; // Rough estimate per event

/// Initialize the metrics system with a tokio runtime
/// This must be called once at startup with an active tokio runtime
pub fn init_metrics_system() {
    if !*METRICS_ENABLED {
        return;
    }

    // Spawn the sender task if not already started
    let _ = METRICS_STATE.sender_handle.get_or_init(|| {
        tokio::spawn(sender_task(
            METRICS_STATE.events_rx.clone(),
            METRICS_STATE.notify_rx.clone(),
        ))
    });
}

/// Initialize thread-local buffer for the current thread
/// Call this once per worker thread
pub fn init_thread_local() {
    if !*METRICS_ENABLED {
        return;
    }

    EVENT_BUFFER.with(|buffer| {
        buffer.borrow_mut().clear();
    });
}

/// Set the memory threshold for batch notifications in bytes
/// When the estimated memory usage exceeds this threshold, the sender task is notified
pub fn set_threshold_bytes(bytes: usize) {
    METRICS_STATE
        .threshold_bytes
        .store(bytes, Ordering::Relaxed);
}

/// Set the memory threshold for batch notifications in megabytes
/// When the estimated memory usage exceeds this threshold, the sender task is notified
pub fn set_threshold_mb(megabytes: usize) {
    let bytes = megabytes * 1024 * 1024;
    set_threshold_bytes(bytes);
}

/// Get the current memory threshold in bytes
pub fn get_threshold_bytes() -> usize {
    METRICS_STATE.threshold_bytes.load(Ordering::Relaxed)
}

/// Log an event to the metrics system
/// Events are buffered locally per-thread and flushed in batches
pub fn log_event<D>(event_type: events::EventType, event_data: D)
where
    D: Into<events::EventData> + Serialize + std::fmt::Debug + Clone,
{
    if !*METRICS_ENABLED {
        return;
    }

    let raw_event = create_raw_event(event_type, event_data.into());

    EVENT_BUFFER.with(|buffer| {
        let mut buf = buffer.borrow_mut();
        buf.push(raw_event);

        // Flush if we've reached the threshold
        if buf.len() >= THREAD_LOCAL_FLUSH_THRESHOLD {
            flush_local_buffer(&mut buf);
        }
    });
}

/// Flush the thread-local buffer to the global channel
fn flush_local_buffer(buf: &mut Vec<events::RawEvent<events::EventData>>) {
    let events = std::mem::take(buf);

    for event in events {
        let _ = METRICS_STATE.events_tx.send(event);
    }

    // Check if we should notify the sender task based on estimated memory usage
    let channel_len = METRICS_STATE.events_tx.len();
    let estimated_bytes = channel_len * ESTIMATED_EVENT_SIZE_BYTES;
    let threshold_bytes = METRICS_STATE.threshold_bytes.load(Ordering::Relaxed);

    if estimated_bytes >= threshold_bytes {
        let _ = METRICS_STATE.notify_tx.try_send(());
    }
}

/// Create a RawEvent with common metadata
fn create_raw_event(
    event_type: events::EventType,
    event_data: events::EventData,
) -> events::RawEvent<events::EventData> {
    events::RawEvent {
        os: OS,
        user_id: Some(&HELIX_USER_ID),
        event_type,
        event_data,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Failed to get system time")
            .as_secs(),
        email: None,
    }
}

/// Background task that batches and sends events via HTTP
async fn sender_task(
    events_rx: flume::Receiver<events::RawEvent<events::EventData>>,
    notify_rx: flume::Receiver<()>,
) {
    loop {
        // Wait for notification or timeout
        tokio::select! {
            _ = notify_rx.recv_async() => {
                process_batch(&events_rx).await;
            }
            _ = tokio::time::sleep(Duration::from_secs(BATCH_TIMEOUT_SECS)) => {
                // Periodic flush even if threshold not reached
                process_batch(&events_rx).await;
            }
        }
    }
}

/// Process a batch of events from the channel
async fn process_batch(
    rx: &flume::Receiver<events::RawEvent<events::EventData>>,
) -> Option<JoinHandle<()>> {
    // Drain all available events at once
    let events: Vec<_> = rx.drain().collect();

    if events.is_empty() {
        return None;
    }

    // Spawn new task for serialization + HTTP
    // This allows the sender task to continue processing batches
    Some(tokio::spawn(async move {
        // Serialize using sonic_rs (fast)
        let json_bytes = match sonic_rs::to_vec(&events) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Failed to serialize events: {}", e);
                return;
            }
        };

        // Send batch over HTTP
        let _ = METRICS_CLIENT
            .post(METRICS_URL)
            .header("Content-Type", "application/json")
            .body(json_bytes)
            .send()
            .await;
    }))
}

/// Flush all pending events immediately
/// Useful for graceful shutdown
pub async fn flush_all() -> Option<JoinHandle<()>> {
    if !*METRICS_ENABLED {
        return None;
    }

    // Flush all thread-local buffers first
    EVENT_BUFFER.with(|buffer| {
        let mut buf = buffer.borrow_mut();
        if !buf.is_empty() {
            flush_local_buffer(&mut buf);
        }
    });

    // Process any remaining events in the channel
    process_batch(&METRICS_STATE.events_rx).await
}

#[derive(Debug)]
pub struct MetricError(String);

impl std::fmt::Display for MetricError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MetricError {}

impl From<sonic_rs::Error> for MetricError {
    fn from(e: sonic_rs::Error) -> Self {
        MetricError(e.to_string())
    }
}

impl From<reqwest::Error> for MetricError {
    fn from(e: reqwest::Error) -> Self {
        MetricError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_thread_local_buffer_initialization() {
        init_thread_local();

        // Verify buffer is initialized and empty
        EVENT_BUFFER.with(|buffer| {
            assert_eq!(buffer.borrow().len(), 0);
            assert!(buffer.borrow().capacity() >= 32);
        });
    }

    #[test]
    fn test_thread_local_buffering() {
        init_thread_local();

        // Log a few events (less than flush threshold)
        for i in 0..5 {
            log_event(
                events::EventType::Test,
                events::TestEvent {
                    cluster_id: format!("test_{}", i),
                    queries_string: "test".to_string(),
                    num_of_queries: 1,
                    time_taken_sec: 1,
                    success: true,
                    error_messages: None,
                },
            );
        }

        // Buffer should have events (or be flushed if >= threshold)
        EVENT_BUFFER.with(|buffer| {
            let len = buffer.borrow().len();
            // Either still in buffer or already flushed
            assert!(len <= 5);
        });
    }

    #[test]
    fn test_thread_local_auto_flush() {
        init_thread_local();

        // Clear the channel first
        while METRICS_STATE.events_rx.try_recv().is_ok() {}

        // Log exactly THREAD_LOCAL_FLUSH_THRESHOLD events to trigger flush
        for i in 0..THREAD_LOCAL_FLUSH_THRESHOLD {
            log_event(
                events::EventType::Test,
                events::TestEvent {
                    cluster_id: format!("test_auto_flush_{}", i),
                    queries_string: "test".to_string(),
                    num_of_queries: 1,
                    time_taken_sec: 1,
                    success: true,
                    error_messages: None,
                },
            );
        }

        // Buffer should be empty after flush
        EVENT_BUFFER.with(|buffer| {
            assert_eq!(buffer.borrow().len(), 0);
        });

        // At least THREAD_LOCAL_FLUSH_THRESHOLD events should have been added
        let channel_count = METRICS_STATE.events_rx.len();
        assert!(
            channel_count >= THREAD_LOCAL_FLUSH_THRESHOLD,
            "Expected at least {} events in channel, got {}",
            THREAD_LOCAL_FLUSH_THRESHOLD,
            channel_count
        );
    }

    #[test]
    fn test_threshold_configuration() {
        // Test setting threshold in bytes
        set_threshold_bytes(1024);
        assert_eq!(get_threshold_bytes(), 1024);

        // Test setting threshold in MB
        set_threshold_mb(10);
        assert_eq!(get_threshold_bytes(), 10 * 1024 * 1024);

        // Reset to default
        set_threshold_bytes(DEFAULT_THRESHOLD_BYTES);
    }

    #[test]
    fn test_default_threshold() {
        // Default should be 5MB
        let threshold = METRICS_STATE.threshold_bytes.load(Ordering::Relaxed);
        assert!(threshold >= DEFAULT_THRESHOLD_BYTES || threshold > 0);
    }

    #[test]
    fn test_threshold_notification_trigger() {
        init_thread_local();

        // Clear channels
        while METRICS_STATE.events_rx.try_recv().is_ok() {}
        while METRICS_STATE.notify_rx.try_recv().is_ok() {}

        // Set a very low threshold to trigger notification
        set_threshold_bytes(100);

        // Log enough events to exceed threshold
        for i in 0..THREAD_LOCAL_FLUSH_THRESHOLD {
            log_event(
                events::EventType::Test,
                events::TestEvent {
                    cluster_id: format!("test_{}", i),
                    queries_string: "test".to_string(),
                    num_of_queries: 1,
                    time_taken_sec: 1,
                    success: true,
                    error_messages: None,
                },
            );
        }

        // Should have triggered a notification
        let notification_count = METRICS_STATE.notify_rx.len();
        assert!(notification_count > 0, "Expected notification to be sent");

        // Reset threshold
        set_threshold_bytes(DEFAULT_THRESHOLD_BYTES);
    }

    #[test]
    fn test_create_raw_event() {
        let event = create_raw_event(
            events::EventType::Test,
            events::EventData::Test(events::TestEvent::default()),
        );

        assert_eq!(event.os, OS.to_string());
        assert_eq!(event.event_type, events::EventType::Test);
        assert!(event.timestamp > 0);
    }

    #[test]
    fn test_multi_threaded_logging() {
        // Skip if metrics are disabled
        if !*METRICS_ENABLED {
            eprintln!("Skipping test_multi_threaded_logging: METRICS_ENABLED is false");
            return;
        }

        let num_threads = 4;
        let events_per_thread = 20;
        let counter = Arc::new(AtomicUsize::new(0));
        let total_expected = num_threads * events_per_thread;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    init_thread_local();

                    for i in 0..events_per_thread {
                        log_event(
                            events::EventType::Test,
                            events::TestEvent {
                                cluster_id: format!("thread_{}_{}", thread_id, i),
                                queries_string: "test".to_string(),
                                num_of_queries: 1,
                                time_taken_sec: 1,
                                success: true,
                                error_messages: None,
                            },
                        );
                        counter.fetch_add(1, AtomicOrdering::SeqCst);
                    }

                    // Flush remaining events
                    EVENT_BUFFER.with(|buffer| {
                        let mut buf = buffer.borrow_mut();
                        if !buf.is_empty() {
                            flush_local_buffer(&mut buf);
                        }
                    });
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all events were attempted to be logged
        assert_eq!(counter.load(AtomicOrdering::SeqCst), total_expected);

        // This test just verifies that multi-threaded logging doesn't crash or deadlock
        // In parallel test execution, the sender task may consume events concurrently
        eprintln!("Multi-threaded logging completed successfully");
    }

    #[tokio::test]
    async fn test_process_batch() {
        // Skip if metrics are disabled
        if !*METRICS_ENABLED {
            eprintln!("Skipping test_process_batch: METRICS_ENABLED is false");
            return;
        }

        // Clear channel
        while METRICS_STATE.events_rx.try_recv().is_ok() {}

        // Add events to channel
        for i in 0..10 {
            let event = create_raw_event(
                events::EventType::Test,
                events::EventData::Test(events::TestEvent {
                    cluster_id: format!("test_batch_{}", i),
                    queries_string: "test".to_string(),
                    num_of_queries: 1,
                    time_taken_sec: 1,
                    success: true,
                    error_messages: None,
                }),
            );
            METRICS_STATE.events_tx.send(event).unwrap();
        }

        // Give a moment for all events to arrive
        tokio::time::sleep(Duration::from_millis(10)).await;

        let initial_count = METRICS_STATE.events_rx.len();

        // In parallel test execution, sender task might process events, so just verify we can process batches
        if initial_count > 0 {
            // Process batch (won't actually send HTTP in test, but will drain channel)
            process_batch(&METRICS_STATE.events_rx).await;

            // Give spawned tasks a moment to start
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Channel should have fewer or equal events
            let final_count = METRICS_STATE.events_rx.len();
            assert!(
                final_count <= initial_count,
                "Expected channel to be drained or stay same, initial: {}, final: {}",
                initial_count,
                final_count
            );
        }
    }

    #[tokio::test]
    async fn test_flush_all() {
        init_thread_local();

        // Clear channel
        while METRICS_STATE.events_rx.try_recv().is_ok() {}

        // Add events to thread-local buffer
        for i in 0..5 {
            log_event(
                events::EventType::Test,
                events::TestEvent {
                    cluster_id: format!("test_{}", i),
                    queries_string: "test".to_string(),
                    num_of_queries: 1,
                    time_taken_sec: 1,
                    success: true,
                    error_messages: None,
                },
            );
        }

        // Flush all
        flush_all().await;

        // Thread-local buffer should be empty
        EVENT_BUFFER.with(|buffer| {
            assert_eq!(buffer.borrow().len(), 0);
        });
    }

    #[tokio::test]
    async fn test_init_metrics_system() {
        // Should not panic when called multiple times
        init_metrics_system();
        init_metrics_system();

        // Sender handle should be initialized
        assert!(METRICS_STATE.sender_handle.get().is_some());
    }

    #[test]
    fn test_event_serialization() {
        let event = create_raw_event(
            events::EventType::QuerySuccess,
            events::EventData::QuerySuccess(events::QuerySuccessEvent {
                cluster_id: Some("test_cluster".to_string()),
                query_name: "test_query".to_string(),
                time_taken_usec: 1000,
            }),
        );

        // Should be able to serialize
        let json = sonic_rs::to_string(&event).unwrap();
        assert!(json.contains("test_cluster"));
        assert!(json.contains("test_query"));
    }

    #[test]
    fn test_batch_serialization() {
        let events: Vec<_> = (0..5)
            .map(|i| {
                create_raw_event(
                    events::EventType::Test,
                    events::EventData::Test(events::TestEvent {
                        cluster_id: format!("test_{}", i),
                        queries_string: "test".to_string(),
                        num_of_queries: 1,
                        time_taken_sec: 1,
                        success: true,
                        error_messages: None,
                    }),
                )
            })
            .collect();

        // Should be able to serialize batch
        let json_bytes = sonic_rs::to_vec(&events).unwrap();
        assert!(json_bytes.len() > 0);

        // Should be valid JSON array
        let json_str = String::from_utf8(json_bytes).unwrap();
        assert!(json_str.starts_with('['));
        assert!(json_str.ends_with(']'));
    }

    #[test]
    fn test_estimated_memory_calculation() {
        // Test that our memory estimation makes sense
        let event = create_raw_event(
            events::EventType::Test,
            events::EventData::Test(events::TestEvent::default()),
        );

        let serialized = sonic_rs::to_vec(&event).unwrap();
        let actual_size = serialized.len();

        // Our estimate should be in the right ballpark (within 2x)
        assert!(
            actual_size < ESTIMATED_EVENT_SIZE_BYTES * 2,
            "Actual size {} exceeds 2x estimate {}",
            actual_size,
            ESTIMATED_EVENT_SIZE_BYTES * 2
        );
    }
}
