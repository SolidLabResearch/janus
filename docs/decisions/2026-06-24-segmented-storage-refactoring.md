# Refactoring of Segmented Storage for Modularity and Readability

Date: 2026-06-24

## Status

Accepted

## Context

The current implementation of segmented storage is contained within a single file, src/storage/segmented_storage.rs, which spans over 880 lines. This file manages multiple distinct concerns:
1. In-memory buffering (BatchBuffer).
2. Segment creation, serialization, and disk-level write operations.
3. Querying, index lookup, and file-based scanning.
4. Background flushing thread loop and error recovery.

Furthermore, there is duplicate logic for creating and writing segment files between the synchronous flush method (create_segment_with_two_level_index) and the background thread flush method (flush_background). This duplication has led to drift, such as the background flushing method omitting the post-processing step to fix max_timestamps in the index blocks, potentially causing missing query hits for events falling into sparse interval gaps.

## Decision

1. Structure src/storage/segmented_storage into a module directory (src/storage/segmented_storage/) with:
   - mod.rs: Struct definitions, lifecycle methods (new, start_background_flushing, shutdown), and write APIs.
   - segment.rs: Disk operations, segment metadata generation, serialization, and index loading.
   - query.rs: Querying interfaces, binary search index block lookup, and data file scanners.
   - background.rs: Background flush loop, background thread tasks, and buffer restoration.
2. Unify segment file creation logic into a single helper method: write_segment_files. This method will sort events, write log and index files, and correctly adjust max_timestamps for index directory blocks.
3. Remove redundant, non-static forwarding methods (serialize_event_to_fixed_size and flush_index_block) and directly call their static counterparts.

## Alternatives Considered

- Keep all implementation in a single file and refactor using impl blocks: Rejected because the file size would remain large and hard to maintain. A directory-based structure enforces cleaner separation of concerns.
- Expose submodules directly to external users: Rejected to preserve public API compatibility. mod.rs will keep all existing public APIs intact.
