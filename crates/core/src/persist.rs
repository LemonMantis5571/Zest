//! Coalescing persistence for thread projections.
//!
//! User / tool / approval / terminal events flush immediately. Text and thinking
//! deltas are checkpointed at most every [`DELTA_CHECKPOINT_MS`] milliseconds.
//! Callers await the terminal flush so a completed turn is on disk before the
//! session controller clears busy state.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use crate::error::{HarnessError, Result};
use crate::thread::{Thread, ThreadStore};

/// Maximum delay before a text/thinking delta checkpoint is written.
pub const DELTA_CHECKPOINT_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistPriority {
    /// User message, tool start/result, approval, done/error/cancelled.
    Immediate,
    /// Streaming text / thinking deltas — coalesced.
    Delta,
}

#[allow(clippy::large_enum_variant)]
enum Cmd {
    Upsert {
        thread: Thread,
        priority: PersistPriority,
        ack: Option<oneshot::Sender<Result<()>>>,
    },
    Flush {
        ack: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        ack: oneshot::Sender<()>,
    },
}

/// Background worker that owns a [`ThreadStore`] and coalesces delta writes.
#[derive(Clone)]
pub struct PersistWorker {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl PersistWorker {
    pub fn spawn(workspace_root: impl Into<PathBuf>) -> Result<Self> {
        let root = workspace_root.into();
        let store = ThreadStore::open(&root)?;
        let (tx, rx) = mpsc::unbounded_channel();
        // Prefer the ambient Tokio runtime (desktop async commands). Fall back to
        // a dedicated thread so sync entrypoints can still construct a worker.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(run_worker(store, rx));
            }
            Err(_) => {
                std::thread::Builder::new()
                    .name("zest-persist".into())
                    .spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("persist worker runtime");
                        rt.block_on(run_worker(store, rx));
                    })
                    .map_err(|e| HarnessError::Other(format!("spawn persist worker: {e}")))?;
            }
        }
        Ok(Self { tx })
    }

    /// Enqueue a thread snapshot. When `ack` is needed, use [`Self::save`] /
    /// [`Self::save_and_wait`] instead.
    pub fn enqueue(&self, thread: Thread, priority: PersistPriority) -> Result<()> {
        self.tx
            .send(Cmd::Upsert {
                thread,
                priority,
                ack: None,
            })
            .map_err(|_| HarnessError::Other("persist worker stopped".into()))
    }

    /// Save immediately (or as a delta) and wait for the write to finish.
    pub async fn save_and_wait(&self, thread: Thread, priority: PersistPriority) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(Cmd::Upsert {
                thread,
                priority,
                ack: Some(ack_tx),
            })
            .map_err(|_| HarnessError::Other("persist worker stopped".into()))?;
        ack_rx
            .await
            .map_err(|_| HarnessError::Other("persist worker dropped ack".into()))?
    }

    /// Force any pending coalesced snapshot to disk.
    pub async fn flush(&self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(Cmd::Flush { ack: ack_tx })
            .map_err(|_| HarnessError::Other("persist worker stopped".into()))?;
        ack_rx
            .await
            .map_err(|_| HarnessError::Other("persist worker dropped flush ack".into()))?
    }

    pub async fn shutdown(&self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        let _ = self.tx.send(Cmd::Shutdown { ack: ack_tx });
        let _ = ack_rx.await;
    }
}

async fn run_worker(store: ThreadStore, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let store = Arc::new(store);
    let mut pending: Option<Thread> = None;
    let mut deadline: Option<Instant> = None;
    let mut waiters: Vec<oneshot::Sender<Result<()>>> = Vec::new();

    loop {
        let sleep_for = deadline.map(|d| {
            let now = Instant::now();
            if d <= now {
                Duration::from_millis(0)
            } else {
                d.saturating_duration_since(now)
            }
        });

        tokio::select! {
            biased;

            cmd = rx.recv() => {
                match cmd {
                    None => {
                        let _ = flush_pending(&store, &mut pending, &mut deadline, &mut waiters);
                        break;
                    }
                    Some(Cmd::Shutdown { ack }) => {
                        let _ = flush_pending(&store, &mut pending, &mut deadline, &mut waiters);
                        let _ = ack.send(());
                        break;
                    }
                    Some(Cmd::Flush { ack }) => {
                        let result = flush_pending(&store, &mut pending, &mut deadline, &mut waiters);
                        let _ = ack.send(result);
                    }
                    Some(Cmd::Upsert { thread, priority, ack }) => {
                        match priority {
                            PersistPriority::Immediate => {
                                // Collapse any pending delta into this write.
                                pending = None;
                                deadline = None;
                                let result = store.save(&thread);
                                let for_waiters = clone_result(&result);
                                if let Some(ack) = ack {
                                    let _ = ack.send(result);
                                }
                                for w in waiters.drain(..) {
                                    let _ = w.send(clone_result(&for_waiters));
                                }
                            }
                            PersistPriority::Delta => {
                                pending = Some(thread);
                                if deadline.is_none() {
                                    deadline = Some(
                                        Instant::now()
                                            + Duration::from_millis(DELTA_CHECKPOINT_MS),
                                    );
                                }
                                if let Some(ack) = ack {
                                    waiters.push(ack);
                                }
                            }
                        }
                    }
                }
            }

            _ = async {
                if let Some(dur) = sleep_for {
                    tokio::time::sleep(dur).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }, if deadline.is_some() => {
                let _ = flush_pending(&store, &mut pending, &mut deadline, &mut waiters);
            }
        }
    }
}

fn flush_pending(
    store: &ThreadStore,
    pending: &mut Option<Thread>,
    deadline: &mut Option<Instant>,
    waiters: &mut Vec<oneshot::Sender<Result<()>>>,
) -> Result<()> {
    *deadline = None;
    let Some(thread) = pending.take() else {
        for w in waiters.drain(..) {
            let _ = w.send(Ok(()));
        }
        return Ok(());
    };
    let result = store.save(&thread);
    for w in waiters.drain(..) {
        let _ = w.send(clone_result(&result));
    }
    result
}

fn clone_result(result: &Result<()>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(HarnessError::Other(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread::new_id;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zest-persist-{name}-{}", new_id("tmp")));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn immediate_save_round_trips() {
        let root = scratch("imm");
        let worker = PersistWorker::spawn(&root).unwrap();
        let mut thread = Thread::new();
        thread.apply_user("u1", "hello");
        worker
            .save_and_wait(thread.clone(), PersistPriority::Immediate)
            .await
            .unwrap();
        let loaded = ThreadStore::open(&root).unwrap().load(&thread.id).unwrap();
        assert_eq!(loaded.messages.len(), 1);
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn delta_coalesces_until_flush() {
        let root = scratch("delta");
        let worker = PersistWorker::spawn(&root).unwrap();
        let mut thread = Thread::new();
        thread.apply_user("u1", "hi");
        worker
            .save_and_wait(thread.clone(), PersistPriority::Immediate)
            .await
            .unwrap();

        thread.apply_text_delta("a1", "partial");
        worker
            .enqueue(thread.clone(), PersistPriority::Delta)
            .unwrap();
        // Before checkpoint interval, disk may still lack the delta — flush forces it.
        worker.flush().await.unwrap();
        let loaded = ThreadStore::open(&root).unwrap().load(&thread.id).unwrap();
        match &loaded.messages[1] {
            crate::thread::StoredMessage::Assistant { text, .. } => {
                assert_eq!(text, "partial");
            }
            other => panic!("expected assistant, got {other:?}"),
        }
        worker.shutdown().await;
    }
}
