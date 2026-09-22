//! Retry classification at the index-driver/object-store boundary.

use std::{error, io, iter};

use slatedb::object_store;

/// Rejects permanent object-store failures, including typed wrapped causes.
///
/// Missing immutable data, invalid requests/configuration, denied access and
/// panicked tasks need intervention. Conditional-write races, cancelled tasks
/// and temporary I/O retain backoff. Unknown errors remain retryable: cloud
/// providers can hide both throttling and server failures behind `Generic`,
/// without exposing a public status type. Error messages are not an API.
pub(super) fn is_retryable_object_store(error: &object_store::Error) -> bool {
    !iter::successors(Some(error as &(dyn error::Error + 'static)), |cause| {
        cause.source()
    })
    .any(|cause| {
        cause
            .downcast_ref::<object_store::Error>()
            .is_some_and(|error| {
                matches!(
                    error,
                    object_store::Error::NotFound { .. }
                        | object_store::Error::InvalidPath { .. }
                        | object_store::Error::NotSupported { .. }
                        | object_store::Error::NotImplemented { .. }
                        | object_store::Error::PermissionDenied { .. }
                        | object_store::Error::Unauthenticated { .. }
                        | object_store::Error::UnknownConfigurationKey { .. }
                        | object_store::Error::NotModified { .. }
                )
            })
            || cause.downcast_ref::<io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::PermissionDenied
                        | io::ErrorKind::InvalidInput
                        | io::ErrorKind::InvalidData
                        | io::ErrorKind::Unsupported
                )
            })
            || cause
                .downcast_ref::<tokio::task::JoinError>()
                .is_some_and(tokio::task::JoinError::is_panic)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_store_errors_block_directly_and_through_wrappers() {
        let source = || io::Error::other("fixture").into();
        for error in [
            object_store::Error::NotFound {
                path: "fixture".into(),
                source: source(),
            },
            object_store::Error::InvalidPath {
                source: object_store::path::Path::parse("../fixture").unwrap_err(),
            },
            object_store::Error::NotSupported { source: source() },
            object_store::Error::NotImplemented {
                operation: "put".into(),
                implementer: "fixture".into(),
            },
            object_store::Error::PermissionDenied {
                path: "fixture".into(),
                source: source(),
            },
            object_store::Error::Unauthenticated {
                path: "fixture".into(),
                source: source(),
            },
            object_store::Error::UnknownConfigurationKey {
                store: "fixture",
                key: "invalid".into(),
            },
            object_store::Error::NotModified {
                path: "fixture".into(),
                source: source(),
            },
        ] {
            assert!(!is_retryable_object_store(&error), "{error:?}");
            let wrapped = object_store::Error::Generic {
                store: "wrapper",
                source: Box::new(error),
            };
            assert!(!is_retryable_object_store(&wrapped), "{wrapped:?}");
        }
    }

    #[test]
    fn typed_io_causes_preserve_permanent_and_transient_distinctions() {
        for (kind, retryable) in [
            (io::ErrorKind::NotFound, false),
            (io::ErrorKind::PermissionDenied, false),
            (io::ErrorKind::InvalidInput, false),
            (io::ErrorKind::InvalidData, false),
            (io::ErrorKind::Unsupported, false),
            (io::ErrorKind::TimedOut, true),
            (io::ErrorKind::ConnectionReset, true),
            (io::ErrorKind::ConnectionRefused, true),
            (io::ErrorKind::Interrupted, true),
            (io::ErrorKind::WouldBlock, true),
            (io::ErrorKind::Other, true),
        ] {
            let error = object_store::Error::Generic {
                store: "fixture",
                source: Box::new(io::Error::new(kind, "fixture")),
            };
            assert_eq!(is_retryable_object_store(&error), retryable, "{kind:?}");
            let nested = object_store::Error::Generic {
                store: "wrapper",
                source: Box::new(error),
            };
            assert_eq!(is_retryable_object_store(&nested), retryable, "{kind:?}");
        }
    }

    #[test]
    fn conditional_races_and_opaque_provider_errors_remain_retryable() {
        for error in [
            object_store::Error::AlreadyExists {
                path: "fixture".into(),
                source: io::Error::other("race").into(),
            },
            object_store::Error::Precondition {
                path: "fixture".into(),
                source: io::Error::other("race").into(),
            },
            object_store::Error::Generic {
                store: "fixture",
                source: "opaque provider failure".into(),
            },
        ] {
            assert!(is_retryable_object_store(&error), "{error:?}");
        }
        for kind in [
            object_store::client::HttpErrorKind::Connect,
            object_store::client::HttpErrorKind::Request,
            object_store::client::HttpErrorKind::Timeout,
            object_store::client::HttpErrorKind::Interrupted,
        ] {
            let error = object_store::Error::Generic {
                store: "fixture",
                source: Box::new(object_store::client::HttpError::new(
                    kind,
                    io::Error::other("transport failure"),
                )),
            };
            assert!(is_retryable_object_store(&error), "{kind:?}");
        }
    }

    #[tokio::test]
    async fn panicked_tasks_block_but_cancelled_tasks_retry() {
        let panicked = tokio::spawn(async { panic!("fixture panic") })
            .await
            .unwrap_err();
        assert!(!is_retryable_object_store(
            &object_store::Error::JoinError { source: panicked }
        ));
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let cancelled = task.await.unwrap_err();
        assert!(is_retryable_object_store(&object_store::Error::JoinError {
            source: cancelled,
        }));
    }
}
