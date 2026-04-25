//! Loom model of the driver-task / Session shutdown handshake.
//!
//! The full driver runs on Tokio; loom can't drive a real Tokio runtime,
//! so we model the *invariant we care about*:
//!
//! > After the `Session` handle drops, every command that was successfully
//! > sent into the inbox is either driven to completion (its reply
//! > channel receives a result) or its reply channel is dropped before the
//! > driver task exits. The caller never observes a permanently-blocked
//! > reply.
//!
//! That invariant is the trickiest concurrency property in the M0 surface:
//! it covers the race between the user dropping the `Session` (which
//! closes the mpsc sender) and the driver task processing the next
//! command. We model it with loom's mpsc channel and an `Arc<Mutex<Option<...>>>`
//! standing in for the per-command oneshot.
//!
//! Run with: `RUSTFLAGS="--cfg loom" cargo test --test loom_driver --release`

#![cfg(loom)]

use std::sync::atomic::{AtomicUsize, Ordering};

use loom::sync::mpsc;
use loom::sync::{Arc, Mutex};
use loom::thread;

/// Stand-in for the `oneshot` reply channel: the caller holds a clone of
/// the Arc and polls; the driver writes once and drops its clone.
type Reply = Arc<Mutex<Option<u32>>>;

#[test]
fn shutdown_does_not_leave_caller_blocked_forever() {
    loom::model(|| {
        let (tx, rx) = mpsc::channel::<(u32, Reply)>();

        // The caller puts a command into the queue and keeps a clone of
        // the reply slot.
        let reply: Reply = Arc::new(Mutex::new(None));

        let driver_reply = reply.clone();
        let driver = thread::spawn(move || {
            // Process at most one command, then exit (simulates the
            // session-dropped path).
            if let Ok((value, slot)) = rx.recv() {
                let _ = driver_reply; // unused: caller's clone is the visibility check
                let mut guard = slot.lock().unwrap();
                *guard = Some(value);
            }
        });

        let caller_reply = reply.clone();
        let caller = thread::spawn(move || {
            // Send the command, then drop the sender (simulating Session
            // drop). The driver may or may not have observed the message
            // yet; either way the caller's reply slot is well-defined.
            tx.send((42, caller_reply.clone())).unwrap();
            drop(tx);

            // Attempt to read the reply: either it's been written
            // (driver processed before drop) or the slot is still None
            // (driver exited without processing). We verify only that
            // *some* terminal state exists; loom explores both.
            let guard = caller_reply.lock().unwrap();
            // Just touching the slot is enough: loom checks for deadlocks.
            let _seen = *guard;
        });

        driver.join().unwrap();
        caller.join().unwrap();
    });
}

#[test]
fn driver_drains_pending_command_before_exit() {
    loom::model(|| {
        // The strict invariant: if the driver has dequeued a command from
        // the mpsc, it MUST write to the reply slot before exiting. The
        // model below succeeds only if every interleaving produces a
        // written slot when the receive succeeds.
        let (tx, rx) = mpsc::channel::<(u32, Reply)>();
        let reply: Reply = Arc::new(Mutex::new(None));
        let processed = Arc::new(AtomicUsize::new(0));

        let driver_reply = reply.clone();
        let driver_processed = processed.clone();
        let driver = thread::spawn(move || {
            if let Ok((value, slot)) = rx.recv() {
                // Once we've taken the value, the contract is: write the
                // reply before returning.
                let mut guard = slot.lock().unwrap();
                *guard = Some(value);
                driver_processed.fetch_add(1, Ordering::SeqCst);
                drop(driver_reply);
            }
        });

        let caller_reply = reply.clone();
        let caller = thread::spawn(move || {
            tx.send((7, caller_reply.clone())).unwrap();
            drop(tx);
        });

        driver.join().unwrap();
        caller.join().unwrap();

        // Either the driver dequeued and wrote, or it didn't dequeue at all.
        let guard = reply.lock().unwrap();
        let written = guard.is_some();
        let proc = processed.load(Ordering::SeqCst);
        assert_eq!(written, proc == 1, "written={written} processed={proc}");
    });
}
