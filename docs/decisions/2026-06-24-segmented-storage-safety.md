# Storage Safety and Correctness in Segmented Storage

Date: 2026-06-24

## Status

Accepted

> Historical safety record. Verify any present-day behavior against the current
> segmented-storage implementation and its regression tests.

## Context

The segmented storage engine has two primary concurrency and data integrity bugs:
1. Thread Safety / Race Conditions during Shutdown and Flush:
   The shutdown sequence sets the shutdown signal to true and immediately executes a final cleanup flush before joining the background thread. If the background thread is currently flushing, the two threads will run the flush path concurrently, causing race conditions in directory writes, index log creation, and updating the catalog list. Additionally, there is no serialization mechanism between manual flushes (called by users) and the background thread.
2. File Corruption Hazard in Dictionary Persistence:
   The dictionary is serialized and written directly to dictionary.bin using File::create, which truncates the file. If a crash or parallel write happens mid-way, the dictionary file is lost or corrupted.

## Decision

1. Flush Lock and Shutdown Race Fix:
   - Introduce a new mutex (flush_lock) to serialize all operations that drain the batch buffer, write segment files, update the catalog, or write the dictionary.
   - Enforce lock-ordering: always acquire flush_lock before acquiring locks on the batch buffer, segments vector, or dictionary.
   - Change the shutdown sequence: set the shutdown signal to true, join the background thread, and then run the final cleanup flush. Do not hold flush_lock while joining the thread to avoid deadlocks.
2. Atomic Dictionary Writes:
   - Rewrite Dictionary::save_to_file to write to a temporary file (dictionary.bin.[pid].[counter].tmp) in the same directory.
   - Flush the writer, call sync_all on the file, close it, and then rename it atomically over dictionary.bin.
   - Delete the temporary file on failure.
   - Sync the parent directory best-effort on Unix.

## Alternatives Considered

- Use a reentrant mutex (ReentrantMutex) for the flush synchronization:
  Rejected because standard library Mutex is not reentrant, and adding another external crate dependency is unnecessary. We can safely structure the code to only acquire the flush_lock at the outer entry points or helper functions.
- Write to a fixed dictionary.bin.tmp file:
  Rejected because if multiple instances of storage or parallel processes are running (or tests are running in parallel), they will conflict on the same temporary file. Using process ID and a thread-safe atomic counter ensures unique filenames.
