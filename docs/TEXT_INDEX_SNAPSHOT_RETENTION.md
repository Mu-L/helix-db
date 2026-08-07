# Text-index blob retention contract

Text-index splits are immutable, content-addressed objects. Writers upload a
split before the SlateDB transaction that attaches its manifest reference.

## Visibility

- Upload failure aborts the graph/index transaction.
- Attachment commits graph mutations, BUILD deltas, statistics, manifest rows,
  live-state rows, and version guards atomically.
- A failed or conflicting attachment may leave an unreachable object.
- Readers resolve only split references present in their retained SlateDB view.

## DROP and retention

DROP removes the index metadata asynchronously and reaches `Succeeded`. It does
not delete split objects. A reader that retained an older SlateDB view can
therefore finish reading the referenced immutable objects after DROP commits.

No writer, lifecycle worker, migration, or background task deletes text blobs.
Storage grows monotonically until a separate reclamation protocol is designed.
