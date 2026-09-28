//! Small synchronization helpers.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Lock `mutex`, recovering the guard even if the lock was poisoned.
///
/// The mutexes in this crate guard a log-file handle and the IPC status/queue
/// snapshots. A panic while one is held would otherwise turn into a second
/// panic on the next access; recovering is safe here because the guarded data
/// is either append-only (the log file) or fully replaced under the lock (the
/// snapshots).
pub fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
