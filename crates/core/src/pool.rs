//! Connection pooling for [`Session`].

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use async_fn_traits::AsyncFnOnce1;
use futures_core::Stream;
use tokio::sync::{Mutex, Notify};

use crate::error::{Error, Result};
use crate::query::{Command, Query};
use crate::{Config, PreparedCommand, PreparedQuery, RowStream, Session};

const HEALTHCHECK_PING_SQL: &str = "SELECT 1";
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);
static SAVEPOINT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Connection-health check run when an idle connection is acquired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthCheck {
    /// Do not run an acquire-time check.
    None,
    /// Run `SELECT 1` before handing the connection to the caller.
    Ping,
    /// Run the supplied SQL string. This uses the simple-query protocol so it
    /// can contain more than one statement.
    ResetQuery(String),
}

/// Pool sizing and lifetime configuration.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    min_idle: usize,
    max_size: usize,
    acquire_timeout: Duration,
    idle_timeout: Option<Duration>,
    max_lifetime: Option<Duration>,
    health_check: HealthCheck,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_idle: 0,
            max_size: 16,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
            idle_timeout: None,
            max_lifetime: None,
            health_check: HealthCheck::None,
        }
    }
}

impl PoolConfig {
    /// Create pool options with conservative defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep at least this many idle connections warm when possible.
    #[must_use]
    pub fn min_idle(mut self, min_idle: usize) -> Self {
        self.min_idle = min_idle;
        self
    }

    /// Maximum number of live connections, including checked-out ones.
    #[must_use]
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// How long [`Pool::acquire`] waits before returning [`PoolError::Timeout`].
    #[must_use]
    pub fn acquire_timeout(mut self, acquire_timeout: Duration) -> Self {
        self.acquire_timeout = acquire_timeout;
        self
    }

    /// Close idle connections once they have been unused for this long.
    #[must_use]
    pub fn idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.idle_timeout = Some(idle_timeout);
        self
    }

    /// Close connections after they reach this total age.
    #[must_use]
    pub fn max_lifetime(mut self, max_lifetime: Duration) -> Self {
        self.max_lifetime = Some(max_lifetime);
        self
    }

    /// Configure the acquire-time health check.
    #[must_use]
    pub fn health_check(mut self, health_check: HealthCheck) -> Self {
        self.health_check = health_check;
        self
    }

    fn validate(&self) -> Result<()> {
        if self.max_size == 0 {
            return Err(Error::Config(
                "pool max_size must be greater than zero".into(),
            ));
        }
        if self.min_idle > self.max_size {
            return Err(Error::Config(
                "pool min_idle cannot be greater than max_size".into(),
            ));
        }
        if self.acquire_timeout.is_zero() {
            return Err(Error::Config(
                "pool acquire_timeout must be greater than zero".into(),
            ));
        }
        if matches!(self.idle_timeout, Some(timeout) if timeout.is_zero()) {
            return Err(Error::Config(
                "pool idle_timeout must be greater than zero".into(),
            ));
        }
        if matches!(self.max_lifetime, Some(timeout) if timeout.is_zero()) {
            return Err(Error::Config(
                "pool max_lifetime must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

/// Errors returned by [`Pool::acquire`].
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// No connection became available before `acquire_timeout` elapsed.
    #[error("pool acquire timed out")]
    Timeout,
    /// The pool was explicitly closed.
    #[error("pool is closed")]
    PoolClosed,
    /// Opening or validating a connection failed.
    #[error("pool acquire failed: {0}")]
    AcquireFailed(Error),
}

/// A connection pool for [`Session`] values.
#[derive(Debug, Clone)]
pub struct Pool {
    inner: Arc<PoolInner>,
}

#[derive(Debug)]
struct PoolInner {
    connect_config: Config,
    pool_config: PoolConfig,
    state: Mutex<PoolState>,
    notify: Notify,
    closed: AtomicBool,
}

#[derive(Debug, Default)]
struct PoolState {
    total: usize,
    idle: VecDeque<IdleSession>,
}

#[derive(Debug)]
struct IdleSession {
    session: Session,
    created_at: Instant,
    idle_since: Instant,
}

/// A checked-out pooled connection.
#[derive(Debug)]
pub struct PoolConnection {
    inner: Arc<PoolInner>,
    session: Option<Session>,
    created_at: Instant,
    broken: Arc<AtomicBool>,
}

/// A pooled prepared query that cannot outlive the checkout it came from.
#[derive(Debug)]
pub struct PooledPreparedQuery<'a, A, B> {
    inner: PreparedQuery<A, B>,
    broken: Arc<AtomicBool>,
    _lifetime: PhantomData<&'a ()>,
}

/// A pooled prepared command that cannot outlive the checkout it came from.
#[derive(Debug)]
pub struct PooledPreparedCommand<'a, A> {
    inner: PreparedCommand<A>,
    broken: Arc<AtomicBool>,
    _lifetime: PhantomData<&'a ()>,
}

/// A pooled row stream that keeps its checkout borrowed for the stream's life.
#[derive(Debug)]
pub struct PooledRowStream<'a, B> {
    inner: RowStream<B>,
    broken: Arc<AtomicBool>,
    _lifetime: PhantomData<&'a ()>,
}

/// A scoped top-level transaction handle borrowed from
/// [`PoolConnection::transaction`].
#[derive(Debug)]
pub struct PooledTransaction<'a> {
    session: &'a Session,
    broken: Arc<AtomicBool>,
}

/// A scoped savepoint handle borrowed from [`PooledTransaction::savepoint`] or
/// [`PooledSavepoint::savepoint`].
#[derive(Debug)]
pub struct PooledSavepoint<'a> {
    session: &'a Session,
    broken: Arc<AtomicBool>,
}

#[derive(Debug)]
struct PooledScopeGuard {
    tx: tokio::sync::mpsc::Sender<crate::session::Command>,
    broken: Arc<AtomicBool>,
    action: PooledCleanupAction,
    active: bool,
}

#[derive(Debug, Clone)]
enum PooledCleanupAction {
    Rollback,
    RollbackSavepoint(String),
}

impl Pool {
    /// Build a new pool.
    pub async fn new(connect_config: Config, pool_config: PoolConfig) -> Result<Self> {
        pool_config.validate()?;
        let inner = Arc::new(PoolInner {
            connect_config,
            pool_config,
            state: Mutex::new(PoolState::default()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
        });
        let pool = Self {
            inner: Arc::clone(&inner),
        };
        inner.ensure_min_idle().await?;
        spawn_maintenance(Arc::downgrade(&inner));
        Ok(pool)
    }

    /// Acquire a connection from the pool.
    pub async fn acquire(&self) -> std::result::Result<PoolConnection, PoolError> {
        let deadline = Instant::now() + self.inner.pool_config.acquire_timeout;
        loop {
            if self.inner.closed.load(Ordering::Relaxed) {
                return Err(PoolError::PoolClosed);
            }

            if let Some(idle) = self.inner.try_take_idle().await {
                if self.inner.is_expired(&idle) {
                    self.inner.discard(idle.session).await;
                    continue;
                }
                if let Err(err) = self.inner.run_health_check(&idle.session).await {
                    self.inner.discard(idle.session).await;
                    if Instant::now() >= deadline {
                        return Err(PoolError::AcquireFailed(err));
                    }
                    continue;
                }
                return Ok(PoolConnection {
                    inner: Arc::clone(&self.inner),
                    session: Some(idle.session),
                    created_at: idle.created_at,
                    broken: Arc::new(AtomicBool::new(false)),
                });
            }

            if self.inner.try_reserve_slot().await {
                match Session::connect_pooled(self.inner.connect_config.clone()).await {
                    Ok(session) => {
                        return Ok(PoolConnection {
                            inner: Arc::clone(&self.inner),
                            session: Some(session),
                            created_at: Instant::now(),
                            broken: Arc::new(AtomicBool::new(false)),
                        });
                    }
                    Err(err) => {
                        self.inner.release_slot().await;
                        return Err(PoolError::AcquireFailed(err));
                    }
                }
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(PoolError::Timeout);
            }
            if tokio::time::timeout(remaining, self.inner.notify.notified())
                .await
                .is_err()
            {
                return Err(PoolError::Timeout);
            }
        }
    }

    /// Close the pool. Idle connections are closed immediately; checked-out
    /// connections are discarded when they return.
    pub async fn close(&self) {
        self.inner.closed.store(true, Ordering::Relaxed);
        self.inner.notify.notify_waiters();
        let idle_sessions = {
            let mut state = self.inner.state.lock().await;
            state.total = state.total.saturating_sub(state.idle.len());
            state
                .idle
                .drain(..)
                .map(|idle| idle.session)
                .collect::<Vec<_>>()
        };
        for session in idle_sessions {
            let _ = session.close().await;
        }
    }
}

impl PoolInner {
    async fn ensure_min_idle(&self) -> Result<()> {
        let target = self.pool_config.min_idle.min(self.pool_config.max_size);
        loop {
            let should_create = {
                let mut state = self.state.lock().await;
                if self.closed.load(Ordering::Relaxed)
                    || state.idle.len() >= target
                    || state.total >= self.pool_config.max_size
                {
                    false
                } else {
                    state.total += 1;
                    true
                }
            };
            if !should_create {
                break;
            }
            match Session::connect_pooled(self.connect_config.clone()).await {
                Ok(session) => {
                    self.return_idle(session, Instant::now(), false).await;
                }
                Err(err) => {
                    self.release_slot().await;
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    async fn try_take_idle(&self) -> Option<IdleSession> {
        let mut state = self.state.lock().await;
        state.idle.pop_front()
    }

    async fn try_reserve_slot(&self) -> bool {
        let mut state = self.state.lock().await;
        if state.total >= self.pool_config.max_size {
            return false;
        }
        state.total += 1;
        true
    }

    async fn release_slot(&self) {
        let mut state = self.state.lock().await;
        state.total = state.total.saturating_sub(1);
        drop(state);
        self.notify.notify_waiters();
    }

    fn is_expired(&self, idle: &IdleSession) -> bool {
        if let Some(idle_timeout) = self.pool_config.idle_timeout {
            if idle.idle_since.elapsed() >= idle_timeout {
                return true;
            }
        }
        if let Some(max_lifetime) = self.pool_config.max_lifetime {
            if idle.created_at.elapsed() >= max_lifetime {
                return true;
            }
        }
        false
    }

    async fn discard(&self, session: Session) {
        self.release_slot().await;
        drop(session);
    }

    async fn recycle(&self, session: Session, created_at: Instant, broken: bool) {
        let mut should_discard = broken || self.closed.load(Ordering::Relaxed);
        if !should_discard
            && session.transaction_status() != b'I'
            && session.run_control_command("ROLLBACK").await.is_err()
        {
            should_discard = true;
        }
        self.return_idle(session, created_at, should_discard).await;
    }

    async fn return_idle(&self, session: Session, created_at: Instant, should_discard: bool) {
        if should_discard {
            self.discard(session).await;
            return;
        }

        let idle = IdleSession {
            session,
            created_at,
            idle_since: Instant::now(),
        };
        if self.is_expired(&idle) {
            self.discard(idle.session).await;
            return;
        }

        let should_store = {
            let mut state = self.state.lock().await;
            if self.closed.load(Ordering::Relaxed) {
                state.total = state.total.saturating_sub(1);
                false
            } else {
                state.idle.push_back(idle);
                true
            }
        };
        if should_store {
            self.notify.notify_one();
        } else {
            self.notify.notify_waiters();
        }
    }

    async fn run_health_check(&self, session: &Session) -> Result<()> {
        match &self.pool_config.health_check {
            HealthCheck::None => Ok(()),
            HealthCheck::Ping => session
                .simple_query_raw(HEALTHCHECK_PING_SQL)
                .await
                .map(|_| ()),
            HealthCheck::ResetQuery(sql) => session.simple_query_raw(sql).await.map(|_| ()),
        }
    }

    async fn maintenance_tick(&self) {
        let expired = {
            let mut state = self.state.lock().await;
            let mut expired = Vec::new();
            let mut keep = VecDeque::with_capacity(state.idle.len());
            while let Some(idle) = state.idle.pop_front() {
                if self.is_expired(&idle) {
                    state.total = state.total.saturating_sub(1);
                    expired.push(idle.session);
                } else {
                    keep.push_back(idle);
                }
            }
            state.idle = keep;
            expired
        };
        for session in expired {
            drop(session);
        }
        let _ = self.ensure_min_idle().await;
    }
}

impl PoolConnection {
    /// Run raw SQL through the simple-query protocol.
    pub async fn simple_query_raw(&self, sql: &str) -> Result<Vec<crate::session::RawRows>> {
        let result = self.session().simple_query_raw(sql).await;
        self.record_result(&result);
        result
    }

    /// Execute a typed command on the checked-out connection.
    pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64> {
        let result = self.session().execute(cmd, args).await;
        self.record_result(&result);
        result
    }

    /// Execute a typed query on the checked-out connection.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        let result = self.session().query(query, args).await;
        self.record_result(&result);
        result
    }

    /// Stream rows using the default batch size.
    pub async fn stream<'a, A, B>(
        &'a self,
        query: &Query<A, B>,
        args: A,
    ) -> Result<PooledRowStream<'a, B>>
    where
        B: Send + 'static,
    {
        let result = self.session().stream(query, args).await;
        self.wrap_stream(result)
    }

    /// Stream rows with an explicit batch size.
    pub async fn stream_with_batch_size<'a, A, B>(
        &'a self,
        query: &Query<A, B>,
        args: A,
        batch_rows: usize,
    ) -> Result<PooledRowStream<'a, B>>
    where
        B: Send + 'static,
    {
        let result = self
            .session()
            .stream_with_batch_size(query, args, batch_rows)
            .await;
        self.wrap_stream(result)
    }

    /// Prepare a query on this connection.
    pub async fn prepare_query<'a, A, B>(
        &'a self,
        query: &Query<A, B>,
    ) -> Result<PooledPreparedQuery<'a, A, B>>
    where
        A: 'static,
        B: 'static,
    {
        let result = self.session().prepare_query(query).await;
        self.wrap_prepared_query(result)
    }

    /// Prepare a command on this connection.
    pub async fn prepare_command<'a, A>(
        &'a self,
        cmd: &Command<A>,
    ) -> Result<PooledPreparedCommand<'a, A>>
    where
        A: 'static,
    {
        let result = self.session().prepare_command(cmd).await;
        self.wrap_prepared_command(result)
    }

    /// Return the backend PID/secret pair for the checked-out connection.
    pub fn backend_key(&self) -> Option<(i32, i32)> {
        self.session().backend_key()
    }

    /// Run work inside a transaction on the checked-out connection.
    pub async fn transaction<T>(
        &self,
        f: impl for<'tx> AsyncFnOnce1<PooledTransaction<'tx>, Output = Result<T>>,
    ) -> Result<T> {
        let mut guard = PooledScopeGuard::top_level(self.session(), Arc::clone(&self.broken));
        self.session().run_control_command("BEGIN").await?;
        let tx = PooledTransaction {
            session: self.session(),
            broken: Arc::clone(&self.broken),
        };
        let result = f(tx).await;
        match result {
            Ok(value) => {
                self.session().run_control_command("COMMIT").await?;
                guard.disarm();
                Ok(value)
            }
            Err(err) => {
                let rollback_result = self.session().run_control_command("ROLLBACK").await;
                guard.disarm();
                rollback_result?;
                self.record_error(&err);
                Err(err)
            }
        }
    }

    fn session(&self) -> &Session {
        self.session
            .as_ref()
            .expect("pooled connection should hold a live session")
    }

    fn wrap_stream<'a, B>(&self, result: Result<RowStream<B>>) -> Result<PooledRowStream<'a, B>> {
        match result {
            Ok(inner) => Ok(PooledRowStream {
                inner,
                broken: Arc::clone(&self.broken),
                _lifetime: PhantomData,
            }),
            Err(err) => {
                self.record_error(&err);
                Err(err)
            }
        }
    }

    fn wrap_prepared_query<'a, A, B>(
        &self,
        result: Result<PreparedQuery<A, B>>,
    ) -> Result<PooledPreparedQuery<'a, A, B>> {
        match result {
            Ok(inner) => Ok(PooledPreparedQuery {
                inner,
                broken: Arc::clone(&self.broken),
                _lifetime: PhantomData,
            }),
            Err(err) => {
                self.record_error(&err);
                Err(err)
            }
        }
    }

    fn wrap_prepared_command<'a, A>(
        &self,
        result: Result<PreparedCommand<A>>,
    ) -> Result<PooledPreparedCommand<'a, A>> {
        match result {
            Ok(inner) => Ok(PooledPreparedCommand {
                inner,
                broken: Arc::clone(&self.broken),
                _lifetime: PhantomData,
            }),
            Err(err) => {
                self.record_error(&err);
                Err(err)
            }
        }
    }

    fn record_result<T>(&self, result: &Result<T>) {
        if let Err(err) = result {
            self.record_error(err);
        }
    }

    fn record_error(&self, err: &Error) {
        if should_recycle(err) {
            self.broken.store(true, Ordering::Relaxed);
        }
    }
}

impl Drop for PoolConnection {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        let inner = Arc::clone(&self.inner);
        let created_at = self.created_at;
        let broken = self.broken.load(Ordering::Relaxed);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                inner.recycle(session, created_at, broken).await;
            });
        }
    }
}

impl<A, B> PooledPreparedQuery<'_, A, B> {
    /// Execute the prepared query.
    pub async fn query(&self, args: A) -> Result<Vec<B>> {
        let result = self.inner.query(args).await;
        if let Err(err) = &result {
            mark_broken(&self.broken, err);
        }
        result
    }

    /// Server-side prepared statement name.
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Explicitly close the prepared statement.
    pub async fn close(self) -> Result<()> {
        let broken = Arc::clone(&self.broken);
        let result = self.inner.close().await;
        if let Err(err) = &result {
            mark_broken(&broken, err);
        }
        result
    }
}

impl<A> PooledPreparedCommand<'_, A> {
    /// Execute the prepared command.
    pub async fn execute(&self, args: A) -> Result<u64> {
        let result = self.inner.execute(args).await;
        if let Err(err) = &result {
            mark_broken(&self.broken, err);
        }
        result
    }

    /// Server-side prepared statement name.
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Explicitly close the prepared statement.
    pub async fn close(self) -> Result<()> {
        let broken = Arc::clone(&self.broken);
        let result = self.inner.close().await;
        if let Err(err) = &result {
            mark_broken(&broken, err);
        }
        result
    }
}

impl<B> PooledRowStream<'_, B> {
    /// Stop receiving more rows.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<B> Stream for PooledRowStream<'_, B> {
    type Item = Result<B>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Err(err))) => {
                mark_broken(&self.broken, &err);
                Poll::Ready(Some(Err(err)))
            }
            other => other,
        }
    }
}

macro_rules! impl_pooled_scope_methods {
    ($ty:ident) => {
        impl $ty<'_> {
            /// Run raw SQL through the simple-query protocol.
            pub async fn simple_query_raw(
                &self,
                sql: &str,
            ) -> Result<Vec<crate::session::RawRows>> {
                let result = self.session.simple_query_raw(sql).await;
                if let Err(err) = &result {
                    mark_broken(&self.broken, err);
                }
                result
            }

            /// Execute a typed command.
            pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64> {
                let result = self.session.execute(cmd, args).await;
                if let Err(err) = &result {
                    mark_broken(&self.broken, err);
                }
                result
            }

            /// Execute a typed query.
            pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
                let result = self.session.query(query, args).await;
                if let Err(err) = &result {
                    mark_broken(&self.broken, err);
                }
                result
            }

            /// Stream rows using the default batch size.
            pub async fn stream<'a, A, B>(
                &'a self,
                query: &Query<A, B>,
                args: A,
            ) -> Result<PooledRowStream<'a, B>>
            where
                B: Send + 'static,
            {
                match self.session.stream(query, args).await {
                    Ok(inner) => Ok(PooledRowStream {
                        inner,
                        broken: Arc::clone(&self.broken),
                        _lifetime: PhantomData,
                    }),
                    Err(err) => {
                        mark_broken(&self.broken, &err);
                        Err(err)
                    }
                }
            }

            /// Stream rows with an explicit batch size.
            pub async fn stream_with_batch_size<'a, A, B>(
                &'a self,
                query: &Query<A, B>,
                args: A,
                batch_rows: usize,
            ) -> Result<PooledRowStream<'a, B>>
            where
                B: Send + 'static,
            {
                match self
                    .session
                    .stream_with_batch_size(query, args, batch_rows)
                    .await
                {
                    Ok(inner) => Ok(PooledRowStream {
                        inner,
                        broken: Arc::clone(&self.broken),
                        _lifetime: PhantomData,
                    }),
                    Err(err) => {
                        mark_broken(&self.broken, &err);
                        Err(err)
                    }
                }
            }

            /// Prepare a query within this scope.
            pub async fn prepare_query<'a, A, B>(
                &'a self,
                query: &Query<A, B>,
            ) -> Result<PooledPreparedQuery<'a, A, B>>
            where
                A: 'static,
                B: 'static,
            {
                match self.session.prepare_query(query).await {
                    Ok(inner) => Ok(PooledPreparedQuery {
                        inner,
                        broken: Arc::clone(&self.broken),
                        _lifetime: PhantomData,
                    }),
                    Err(err) => {
                        mark_broken(&self.broken, &err);
                        Err(err)
                    }
                }
            }

            /// Prepare a command within this scope.
            pub async fn prepare_command<'a, A>(
                &'a self,
                cmd: &Command<A>,
            ) -> Result<PooledPreparedCommand<'a, A>>
            where
                A: 'static,
            {
                match self.session.prepare_command(cmd).await {
                    Ok(inner) => Ok(PooledPreparedCommand {
                        inner,
                        broken: Arc::clone(&self.broken),
                        _lifetime: PhantomData,
                    }),
                    Err(err) => {
                        mark_broken(&self.broken, &err);
                        Err(err)
                    }
                }
            }
        }
    };
}

impl_pooled_scope_methods!(PooledTransaction);
impl_pooled_scope_methods!(PooledSavepoint);

impl PooledTransaction<'_> {
    /// Nest another savepoint under this transaction.
    pub async fn savepoint<T>(
        &self,
        f: impl for<'sp> AsyncFnOnce1<PooledSavepoint<'sp>, Output = Result<T>>,
    ) -> Result<T> {
        run_pooled_savepoint(self.session, &self.broken, f).await
    }
}

impl PooledSavepoint<'_> {
    /// Nest another savepoint under this savepoint.
    pub async fn savepoint<T>(
        &self,
        f: impl for<'sp> AsyncFnOnce1<PooledSavepoint<'sp>, Output = Result<T>>,
    ) -> Result<T> {
        run_pooled_savepoint(self.session, &self.broken, f).await
    }
}

async fn run_pooled_savepoint<T>(
    session: &Session,
    broken: &Arc<AtomicBool>,
    f: impl for<'sp> AsyncFnOnce1<PooledSavepoint<'sp>, Output = Result<T>>,
) -> Result<T> {
    let name = next_savepoint_name();
    let mut guard = PooledScopeGuard::savepoint(session, Arc::clone(broken), name.clone());
    session
        .run_control_command(&format!("SAVEPOINT {name}"))
        .await?;
    let savepoint = PooledSavepoint {
        session,
        broken: Arc::clone(broken),
    };
    let result = f(savepoint).await;
    match result {
        Ok(value) => {
            session
                .run_control_command(&format!("RELEASE SAVEPOINT {name}"))
                .await?;
            guard.disarm();
            Ok(value)
        }
        Err(err) => {
            rollback_savepoint(session, &name).await?;
            guard.disarm();
            mark_broken(broken, &err);
            Err(err)
        }
    }
}

impl PooledScopeGuard {
    fn top_level(session: &Session, broken: Arc<AtomicBool>) -> Self {
        Self {
            tx: session.command_tx(),
            broken,
            action: PooledCleanupAction::Rollback,
            active: true,
        }
    }

    fn savepoint(session: &Session, broken: Arc<AtomicBool>, name: String) -> Self {
        Self {
            tx: session.command_tx(),
            broken,
            action: PooledCleanupAction::RollbackSavepoint(name),
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for PooledScopeGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.tx.clone();
        let broken = Arc::clone(&self.broken);
        let action = self.action.clone();
        handle.spawn(async move {
            match action {
                PooledCleanupAction::Rollback => {
                    if crate::session::run_control_command_with_tx(&tx, "ROLLBACK")
                        .await
                        .is_err()
                    {
                        broken.store(true, Ordering::Relaxed);
                    }
                }
                PooledCleanupAction::RollbackSavepoint(name) => {
                    let rollback = crate::session::run_control_command_with_tx(
                        &tx,
                        &format!("ROLLBACK TO SAVEPOINT {name}"),
                    )
                    .await;
                    let release = crate::session::run_control_command_with_tx(
                        &tx,
                        &format!("RELEASE SAVEPOINT {name}"),
                    )
                    .await;
                    if rollback.is_err() || release.is_err() {
                        broken.store(true, Ordering::Relaxed);
                    }
                }
            }
        });
    }
}

fn spawn_maintenance(inner: Weak<PoolInner>) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            loop {
                tokio::time::sleep(MAINTENANCE_INTERVAL).await;
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                if inner.closed.load(Ordering::Relaxed) {
                    return;
                }
                inner.maintenance_tick().await;
            }
        });
    }
}

fn should_recycle(err: &Error) -> bool {
    match err {
        Error::Closed { .. } | Error::Io(_) | Error::Protocol(_) => true,
        Error::Server { code, .. } => {
            code.starts_with("08") || matches!(code.as_str(), "26000" | "08P01")
        }
        Error::Auth(_)
        | Error::UnsupportedAuth(_)
        | Error::Config(_)
        | Error::Codec(_)
        | Error::Migration(_)
        | Error::ColumnAlignment { .. }
        | Error::SchemaMismatch { .. } => false,
    }
}

fn mark_broken(flag: &AtomicBool, err: &Error) {
    if should_recycle(err) {
        flag.store(true, Ordering::Relaxed);
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
    format!("babar_pool_sp_{id}")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{HealthCheck, PoolConfig};

    #[test]
    fn test_pool_config_defaults_are_sane() {
        let cfg = PoolConfig::default();
        assert!(cfg.min_idle <= cfg.max_size);
    }

    #[test]
    fn test_pool_config_rejects_zero_max_size() {
        let err = PoolConfig::new().max_size(0).validate().unwrap_err();
        assert!(err.to_string().contains("max_size"));
    }

    #[test]
    fn test_pool_config_rejects_min_idle_above_max() {
        let err = PoolConfig::new()
            .min_idle(2)
            .max_size(1)
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("min_idle"));
    }

    #[test]
    fn test_pool_config_rejects_zero_timeouts() {
        let err = PoolConfig::new()
            .acquire_timeout(Duration::ZERO)
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("acquire_timeout"));

        let err = PoolConfig::new()
            .idle_timeout(Duration::ZERO)
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("idle_timeout"));

        let err = PoolConfig::new()
            .max_lifetime(Duration::ZERO)
            .validate()
            .unwrap_err();
        assert!(err.to_string().contains("max_lifetime"));
    }

    #[test]
    fn test_health_check_reset_query_preserves_sql() {
        let cfg = PoolConfig::new().health_check(HealthCheck::ResetQuery("DISCARD TEMP".into()));
        match cfg.health_check {
            HealthCheck::ResetQuery(sql) => assert_eq!(sql, "DISCARD TEMP"),
            HealthCheck::None | HealthCheck::Ping => panic!("expected reset query health check"),
        }
    }
}
