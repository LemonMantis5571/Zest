//! Session controller: monotonic session identity, one active turn, cancel.
//!
//! Replaces `Mutex<Option<Session>>` + `AtomicBool`. An old turn finishing after
//! `end_session` / a newer session must never restore into the live slot.
//!
//! Detached / ended turns stay registered until they quiesce (`finish_turn`) so
//! cancel and busy checks remain authoritative until the worker exits.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use zest_core::{new_id, Agent, CancelToken, RecoverableRun, SkillSet, Thread};

pub struct Session {
    pub session_id: String,
    pub agent: Agent,
    pub model: String,
    pub effort: String,
    pub provider_id: String,
    pub provider_label: String,
    pub root: PathBuf,
    pub thread_id: String,
    pub thread: Thread,
    /// A previous process left a provider turn unfinished. The prompt remains
    /// in `thread.messages`; this identity lets the UI offer a fresh retry
    /// without claiming the provider can resume its old stream.
    pub recovery: Option<RecoverableRun>,
    /// Front-end base prompt (before custom + skills layers).
    pub base_system: String,
    /// Shared with `read_skill` for hot-reload.
    pub skills: Arc<RwLock<SkillSet>>,
}

#[derive(Clone)]
pub struct ActiveTurn {
    pub turn_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub root: PathBuf,
    pub cancel: CancelToken,
}

struct Inner {
    next_seq: u64,
    /// Session the UI currently owns. Cleared by `end_session`.
    live_session_id: Option<String>,
    /// Idle session body. Absent while a turn holds it or when empty.
    session: Option<Session>,
    /// In-flight turn. Stays set until [`SessionController::finish_turn`] even
    /// after `end_session` cancelled it (quiesce).
    turn: Option<ActiveTurn>,
    /// When true, `finish_turn` must not restore the session body.
    session_ended: bool,
}

pub struct SessionController {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
pub enum SessionError {
    Busy,
    NoSession,
    Poisoned,
}

impl SessionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::NoSession => "no_session",
            Self::Poisoned => "poisoned",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            // Reaches the UI verbatim, so it has to say what to do rather than
            // describe internal state.
            Self::Busy => "the assistant is still working — stop it or wait for it to finish",
            Self::NoSession => "no active session — choose a provider first",
            Self::Poisoned => "session lock poisoned",
        }
    }
}

impl SessionController {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_seq: 1,
                live_session_id: None,
                session: None,
                turn: None,
                session_ended: false,
            }),
        }
    }

    pub fn is_busy(&self) -> Result<bool, SessionError> {
        let g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        Ok(g.turn.is_some())
    }

    pub fn require_idle(&self) -> Result<(), SessionError> {
        if self.is_busy()? {
            Err(SessionError::Busy)
        } else {
            Ok(())
        }
    }

    pub fn set_session(&self, mut session: Session) -> Result<(), SessionError> {
        let mut g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if g.turn.is_some() {
            return Err(SessionError::Busy);
        }
        let seq = g.next_seq;
        g.next_seq = g.next_seq.saturating_add(1);
        let session_id = format!("session-{seq}");
        session.session_id = session_id.clone();
        g.live_session_id = Some(session_id);
        g.session = Some(session);
        g.session_ended = false;
        Ok(())
    }

    pub fn with_session_mut<R>(
        &self,
        f: impl FnOnce(&mut Session) -> R,
    ) -> Result<R, SessionError> {
        let mut g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if g.turn.is_some() {
            return Err(SessionError::Busy);
        }
        let session = g.session.as_mut().ok_or(SessionError::NoSession)?;
        Ok(f(session))
    }

    pub fn session_info_snapshot<R>(
        &self,
        f: impl FnOnce(&Session) -> R,
    ) -> Result<Option<R>, SessionError> {
        let g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        Ok(g.session.as_ref().map(f))
    }

    /// Take the session for a turn. Records active turn metadata for cancel.
    pub fn begin_turn(&self) -> Result<(Session, ActiveTurn), SessionError> {
        let mut g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if g.turn.is_some() {
            return Err(SessionError::Busy);
        }
        let session = g.session.take().ok_or(SessionError::NoSession)?;
        let turn = ActiveTurn {
            turn_id: new_id("turn"),
            session_id: session.session_id.clone(),
            thread_id: session.thread_id.clone(),
            root: session.root.clone(),
            cancel: CancelToken::new(),
        };
        g.turn = Some(turn.clone());
        Ok((session, turn))
    }

    /// Cancel the active turn token. Does not clear the turn slot — that waits
    /// for [`Self::finish_turn`] (quiesce).
    pub fn cancel_turn(&self) -> Result<bool, SessionError> {
        let g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if let Some(turn) = &g.turn {
            turn.cancel.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Snapshot the active turn for lifecycle persistence and UI commands that
    /// need the project root while the session body is temporarily in flight.
    pub fn active_turn(&self) -> Result<Option<ActiveTurn>, SessionError> {
        let g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        Ok(g.turn.clone())
    }

    /// Return the session after a turn. No-ops when the live session changed
    /// (end/start) so a stale turn cannot overwrite newer state.
    pub fn finish_turn(&self, turn: &ActiveTurn, session: Session) -> Result<bool, SessionError> {
        let mut g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if g.turn.as_ref().is_some_and(|t| t.turn_id == turn.turn_id) {
            g.turn = None;
        }
        let restore = !g.session_ended
            && g.live_session_id.as_deref() == Some(turn.session_id.as_str())
            && g.session.is_none();
        if restore {
            g.session = Some(session);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn end_session(&self) -> Result<(), SessionError> {
        let mut g = self.inner.lock().map_err(|_| SessionError::Poisoned)?;
        if let Some(turn) = &g.turn {
            turn.cancel.cancel();
            // Keep `turn` registered until the worker calls finish_turn.
        }
        g.session_ended = true;
        g.live_session_id = None;
        g.session = None;
        Ok(())
    }
}

impl Default for SessionController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use zest_core::HarnessError;
    use zest_core::{AuthStatus, Completion, StreamEvent, TurnRequest};
    use zest_core::{Provider, ToolRegistry};

    struct StubProvider;

    #[async_trait]
    impl Provider for StubProvider {
        fn id(&self) -> &str {
            "stub"
        }
        fn default_model(&self) -> &str {
            "stub"
        }
        fn auth_status(&self) -> AuthStatus {
            AuthStatus::Ready { account: None }
        }
        async fn stream_turn(
            &self,
            _req: &TurnRequest,
            _on_event: &mut (dyn for<'a> FnMut(StreamEvent<'a>) + Send),
        ) -> zest_core::Result<Completion> {
            Err(HarnessError::Other("unused".into()))
        }
    }

    fn dummy_session(id_suffix: &str) -> Session {
        use std::sync::RwLock;
        use zest_core::SkillSet;
        let provider: Arc<dyn Provider> = Arc::new(StubProvider);
        Session {
            session_id: String::new(),
            agent: Agent::new(provider, ToolRegistry::new()),
            model: "m".into(),
            effort: "high".into(),
            provider_id: "stub".into(),
            provider_label: "Stub".into(),
            root: PathBuf::from("."),
            thread_id: format!("thread-{id_suffix}"),
            thread: Thread::new(),
            recovery: None,
            base_system: "test".into(),
            skills: Arc::new(RwLock::new(SkillSet::default())),
        }
    }

    #[test]
    fn finish_turn_does_not_restore_after_end_session() {
        let ctl = SessionController::new();
        ctl.set_session(dummy_session("a")).unwrap();
        let (session, turn) = ctl.begin_turn().unwrap();
        ctl.end_session().unwrap();
        // Turn stays registered until quiesce.
        assert!(ctl.is_busy().unwrap());
        let restored = ctl.finish_turn(&turn, session).unwrap();
        assert!(!restored);
        assert!(!ctl.is_busy().unwrap());
    }

    #[test]
    fn cancel_sets_token() {
        let ctl = SessionController::new();
        ctl.set_session(dummy_session("b")).unwrap();
        let (_session, turn) = ctl.begin_turn().unwrap();
        assert!(ctl.cancel_turn().unwrap());
        assert!(turn.cancel.is_cancelled());
    }
}
