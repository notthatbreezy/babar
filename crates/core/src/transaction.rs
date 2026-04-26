//! Scoped transaction and savepoint support.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

use async_fn_traits::AsyncFnOnce1;
use tracing::Instrument as _;

use crate::error::Result;
use crate::query::{Command, Query};
use crate::session::{PreparedCommand, PreparedQuery, RowStream, Session};
use crate::telemetry;

static SAVEPOINT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A scoped transaction handle borrowed from [`Session::transaction`].
#[derive(Debug)]
pub struct Transaction<'a> {
    session: &'a Session,
    _marker: PhantomData<&'a Session>,
}

impl Session {
    /// Run `f` inside a transaction.
    pub async fn transaction<T>(
        &self,
        f: impl for<'tx> AsyncFnOnce1<Transaction<'tx>, Output = Result<T>>,
    ) -> Result<T> {
        let span = telemetry::transaction_span("transaction");
        async {
            let mut guard = ScopeGuard::top_level(self);
            self.run_control_command("BEGIN").await?;

            let tx = Transaction::new(self);
            match f(tx).await {
                Ok(value) => {
                    self.run_control_command("COMMIT").await?;
                    guard.disarm();
                    Ok(value)
                }
                Err(err) => {
                    let rollback_result = self.run_control_command("ROLLBACK").await;
                    guard.disarm();
                    rollback_result?;
                    Err(err)
                }
            }
        }
        .instrument(span)
        .await
    }
}

impl<'a> Transaction<'a> {
    pub(crate) fn new(session: &'a Session) -> Self {
        Self {
            session,
            _marker: PhantomData,
        }
    }

    /// Run raw SQL through the simple-query protocol.
    pub async fn simple_query_raw(&self, sql: &str) -> Result<Vec<crate::session::RawRows>> {
        self.session.simple_query_raw(sql).await
    }

    /// Execute a typed command inside this transaction.
    pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64> {
        self.session.execute(cmd, args).await
    }

    /// Execute a typed query inside this transaction and collect all rows.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        self.session.query(query, args).await
    }

    /// Stream rows inside this transaction using the default batch size.
    pub async fn stream<A, B>(&self, query: &Query<A, B>, args: A) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        self.session.stream(query, args).await
    }

    /// Stream rows inside this transaction with an explicit batch size.
    pub async fn stream_with_batch_size<A, B>(
        &self,
        query: &Query<A, B>,
        args: A,
        batch_rows: usize,
    ) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        self.session
            .stream_with_batch_size(query, args, batch_rows)
            .await
    }

    /// Prepare a query on the underlying connection.
    pub async fn prepare_query<A, B>(&self, query: &Query<A, B>) -> Result<PreparedQuery<A, B>>
    where
        A: 'static,
        B: 'static,
    {
        self.session.prepare_query(query).await
    }

    /// Prepare a command on the underlying connection.
    pub async fn prepare_command<A>(&self, cmd: &Command<A>) -> Result<PreparedCommand<A>>
    where
        A: 'static,
    {
        self.session.prepare_command(cmd).await
    }

    /// Run `f` inside a savepoint nested under this transaction.
    pub async fn savepoint<T>(
        &self,
        f: impl for<'sp> AsyncFnOnce1<Transaction<'sp>, Output = Result<T>>,
    ) -> Result<T> {
        let span = telemetry::transaction_span("savepoint");
        async {
            let name = next_savepoint_name();
            let mut guard = ScopeGuard::savepoint(self.session, name.clone());
            self.session
                .run_control_command(&format!("SAVEPOINT {name}"))
                .await?;

            let savepoint = Transaction::new(self.session);
            match f(savepoint).await {
                Ok(value) => {
                    self.session
                        .run_control_command(&format!("RELEASE SAVEPOINT {name}"))
                        .await?;
                    guard.disarm();
                    Ok(value)
                }
                Err(err) => {
                    rollback_savepoint(self.session, &name).await?;
                    guard.disarm();
                    Err(err)
                }
            }
        }
        .instrument(span)
        .await
    }
}

#[derive(Debug)]
struct ScopeGuard {
    tx: tokio::sync::mpsc::Sender<crate::session::Command>,
    action: CleanupAction,
    active: bool,
}

#[derive(Debug, Clone)]
enum CleanupAction {
    Rollback,
    RollbackSavepoint(String),
}

impl ScopeGuard {
    fn top_level(session: &Session) -> Self {
        Self {
            tx: session.command_tx(),
            action: CleanupAction::Rollback,
            active: true,
        }
    }

    fn savepoint(session: &Session, name: String) -> Self {
        Self {
            tx: session.command_tx(),
            action: CleanupAction::RollbackSavepoint(name),
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.tx.clone();
        let action = self.action.clone();
        handle.spawn(async move {
            match action {
                CleanupAction::Rollback => {
                    let _ = crate::session::run_control_command_with_tx(&tx, "ROLLBACK").await;
                }
                CleanupAction::RollbackSavepoint(name) => {
                    let _ = crate::session::run_control_command_with_tx(
                        &tx,
                        &format!("ROLLBACK TO SAVEPOINT {name}"),
                    )
                    .await;
                    let _ = crate::session::run_control_command_with_tx(
                        &tx,
                        &format!("RELEASE SAVEPOINT {name}"),
                    )
                    .await;
                }
            }
        });
    }
}

async fn rollback_savepoint(session: &Session, name: &str) -> Result<()> {
    session
        .run_control_command(&format!("ROLLBACK TO SAVEPOINT {name}"))
        .await?;
    session
        .run_control_command(&format!("RELEASE SAVEPOINT {name}"))
        .await
}

fn next_savepoint_name() -> String {
    let id = SAVEPOINT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("babar_sp_{id}")
}

#[cfg(test)]
mod tests {
    use super::next_savepoint_name;

    #[test]
    fn test_savepoint_names_are_unique() {
        let a = next_savepoint_name();
        let b = next_savepoint_name();
        assert_ne!(a, b);
        assert!(a.starts_with("babar_sp_"));
        assert!(b.starts_with("babar_sp_"));
    }
}
