//! Rhai scripting engine with KVM-specific API registrations and sandboxing.

use std::sync::{Arc, Mutex};

use rhai::{Array, Dynamic, Engine, FuncArgs, Scope, AST};
use tokio::sync::mpsc;
use tracing::info;

use s_kvm_core::{DisplayInfo, PeerInfo};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the scripting subsystem.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script compilation error: {0}")]
    Compile(String),
    #[error("script runtime error: {0}")]
    Runtime(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Command / Event enums
// ---------------------------------------------------------------------------

/// Commands that scripts can issue to the KVM system.
#[derive(Debug, Clone)]
pub enum ScriptCommand {
    SwitchToScreen(i64),
    SendClipboard(String),
    LockToScreen,
    UnlockScreen,
    Log(String),
    Notify { title: String, msg: String },
}

/// Events dispatched to scripts.
#[derive(Debug, Clone)]
pub enum ScriptEvent {
    ScreenEnter { peer_name: String, display_id: u32 },
    ScreenLeave { peer_name: String, display_id: u32 },
    PeerConnected { peer_name: String },
    PeerDisconnected { peer_name: String },
}

// ---------------------------------------------------------------------------
// Shared KVM state
// ---------------------------------------------------------------------------

/// KVM state readable by scripts (behind `Arc<Mutex<…>>`).
#[derive(Debug, Clone, Default)]
pub struct KvmState {
    pub peers: Vec<PeerInfo>,
    pub active_peer_name: String,
    pub displays: Vec<DisplayInfo>,
    pub screen_locked: bool,
}

// ---------------------------------------------------------------------------
// Script-facing wrapper types (simplified for Rhai)
// ---------------------------------------------------------------------------

/// Peer info exposed to Rhai scripts.
#[derive(Debug, Clone)]
pub struct ScriptPeerInfo {
    pub id: String,
    pub hostname: String,
    pub os: String,
    pub display_count: i64,
}

impl From<&PeerInfo> for ScriptPeerInfo {
    fn from(p: &PeerInfo) -> Self {
        Self {
            id: p.id.to_string(),
            hostname: p.hostname.clone(),
            os: format!("{:?}", p.os),
            display_count: p.displays.len() as i64,
        }
    }
}

/// Display info exposed to Rhai scripts.
#[derive(Debug, Clone)]
pub struct ScriptDisplayInfo {
    pub id: i64,
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub is_primary: bool,
}

impl From<&DisplayInfo> for ScriptDisplayInfo {
    fn from(d: &DisplayInfo) -> Self {
        Self {
            id: d.id as i64,
            name: d.name.clone(),
            width: d.width as i64,
            height: d.height as i64,
            is_primary: d.is_primary,
        }
    }
}

// ---------------------------------------------------------------------------
// ScriptEngine
// ---------------------------------------------------------------------------

/// The Rhai-based scripting engine for S-KVM.
///
/// Wraps a [`rhai::Engine`] configured with:
/// - Sandboxing limits (max operations, string size, array size)
/// - Custom `PeerInfo` / `DisplayInfo` types with property accessors
/// - KVM API functions (`switch_to_screen`, `get_peers`, `log`, …)
///
/// Scripts communicate with the rest of the system through a
/// [`tokio::sync::mpsc::UnboundedSender<ScriptCommand>`] channel.
pub struct ScriptEngine {
    engine: Engine,
    #[allow(dead_code)]
    state: Arc<Mutex<KvmState>>,
    #[allow(dead_code)]
    command_tx: mpsc::UnboundedSender<ScriptCommand>,
}

impl ScriptEngine {
    /// Create a new script engine.
    ///
    /// * `state`      – shared KVM state that scripts can query
    /// * `command_tx`  – channel for scripts to send commands to the KVM system
    pub fn new(
        state: Arc<Mutex<KvmState>>,
        command_tx: mpsc::UnboundedSender<ScriptCommand>,
    ) -> Self {
        let mut engine = Engine::new();

        // ── Sandboxing ──────────────────────────────────────────────
        engine.set_max_operations(100_000);
        engine.set_max_string_size(10_000);
        engine.set_max_array_size(100);
        // Rhai has no built-in file-system or network access, so the
        // engine is sandboxed by default in those dimensions.

        // ── Custom types ────────────────────────────────────────────
        Self::register_types(&mut engine);

        // ── KVM API functions ───────────────────────────────────────
        Self::register_api(&mut engine, &state, &command_tx);

        Self {
            engine,
            state,
            command_tx,
        }
    }

    // ── Type registration ───────────────────────────────────────────

    fn register_types(engine: &mut Engine) {
        // PeerInfo
        engine.register_type_with_name::<ScriptPeerInfo>("PeerInfo");
        engine.register_get("id", |p: &mut ScriptPeerInfo| p.id.clone());
        engine.register_get("hostname", |p: &mut ScriptPeerInfo| p.hostname.clone());
        engine.register_get("os", |p: &mut ScriptPeerInfo| p.os.clone());
        engine.register_get("display_count", |p: &mut ScriptPeerInfo| p.display_count);
        engine.register_fn("to_string", |p: &mut ScriptPeerInfo| {
            format!("Peer({})", p.hostname)
        });

        // DisplayInfo
        engine.register_type_with_name::<ScriptDisplayInfo>("DisplayInfo");
        engine.register_get("id", |d: &mut ScriptDisplayInfo| d.id);
        engine.register_get("name", |d: &mut ScriptDisplayInfo| d.name.clone());
        engine.register_get("width", |d: &mut ScriptDisplayInfo| d.width);
        engine.register_get("height", |d: &mut ScriptDisplayInfo| d.height);
        engine.register_get("is_primary", |d: &mut ScriptDisplayInfo| d.is_primary);
        engine.register_fn("to_string", |d: &mut ScriptDisplayInfo| {
            format!("Display({}, {}x{})", d.name, d.width, d.height)
        });
    }

    // ── API function registration ───────────────────────────────────

    fn register_api(
        engine: &mut Engine,
        state: &Arc<Mutex<KvmState>>,
        command_tx: &mpsc::UnboundedSender<ScriptCommand>,
    ) {
        // switch_to_screen(index: i64)
        let tx = command_tx.clone();
        engine.register_fn("switch_to_screen", move |index: i64| {
            let _ = tx.send(ScriptCommand::SwitchToScreen(index));
        });

        // get_peers() -> Array
        let st = state.clone();
        engine.register_fn("get_peers", move || -> Array {
            let guard = st.lock().expect("KvmState lock poisoned");
            guard
                .peers
                .iter()
                .map(|p| Dynamic::from(ScriptPeerInfo::from(p)))
                .collect()
        });

        // get_active_peer() -> String
        let st = state.clone();
        engine.register_fn("get_active_peer", move || -> String {
            st.lock()
                .expect("KvmState lock poisoned")
                .active_peer_name
                .clone()
        });

        // send_clipboard(text: String)
        let tx = command_tx.clone();
        engine.register_fn("send_clipboard", move |text: String| {
            let _ = tx.send(ScriptCommand::SendClipboard(text));
        });

        // get_displays() -> Array
        let st = state.clone();
        engine.register_fn("get_displays", move || -> Array {
            let guard = st.lock().expect("KvmState lock poisoned");
            guard
                .displays
                .iter()
                .map(|d| Dynamic::from(ScriptDisplayInfo::from(d)))
                .collect()
        });

        // lock_to_screen()
        let tx = command_tx.clone();
        engine.register_fn("lock_to_screen", move || {
            let _ = tx.send(ScriptCommand::LockToScreen);
        });

        // unlock_screen()
        let tx = command_tx.clone();
        engine.register_fn("unlock_screen", move || {
            let _ = tx.send(ScriptCommand::UnlockScreen);
        });

        // log(msg: String)
        let tx = command_tx.clone();
        engine.register_fn("log", move |msg: String| {
            info!(target: "rhai_script", "{}", msg);
            let _ = tx.send(ScriptCommand::Log(msg));
        });

        // notify(title: String, msg: String)
        let tx = command_tx.clone();
        engine.register_fn("notify", move |title: String, msg: String| {
            let _ = tx.send(ScriptCommand::Notify { title, msg });
        });
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Compile a Rhai source string into an [`AST`].
    pub fn compile(&self, source: &str) -> Result<AST, ScriptError> {
        self.engine
            .compile(source)
            .map_err(|e| ScriptError::Compile(e.to_string()))
    }

    /// Execute the top-level statements of a compiled script.
    pub fn run(&self, ast: &AST) -> Result<(), ScriptError> {
        self.engine
            .eval_ast::<Dynamic>(ast)
            .map(|_| ())
            .map_err(|e| ScriptError::Runtime(e.to_string()))
    }

    /// Dispatch a [`ScriptEvent`] to a compiled script.
    ///
    /// Calls the matching `on_*` function if it exists. If the script does
    /// not define the handler, this returns `Ok(())`.
    pub fn dispatch_event(&self, ast: &AST, event: &ScriptEvent) -> Result<(), ScriptError> {
        match event {
            ScriptEvent::ScreenEnter {
                peer_name,
                display_id,
            } => self.try_call(
                ast,
                "on_screen_enter",
                (peer_name.clone(), *display_id as i64),
            ),
            ScriptEvent::ScreenLeave {
                peer_name,
                display_id,
            } => self.try_call(
                ast,
                "on_screen_leave",
                (peer_name.clone(), *display_id as i64),
            ),
            ScriptEvent::PeerConnected { peer_name } => {
                self.try_call(ast, "on_peer_connected", (peer_name.clone(),))
            }
            ScriptEvent::PeerDisconnected { peer_name } => {
                self.try_call(ast, "on_peer_disconnected", (peer_name.clone(),))
            }
        }
    }

    /// Try to call a script function, silently succeeding if it does not exist.
    fn try_call<A: FuncArgs>(
        &self,
        ast: &AST,
        fn_name: &str,
        args: A,
    ) -> Result<(), ScriptError> {
        let mut scope = Scope::new();
        match self
            .engine
            .call_fn::<Dynamic>(&mut scope, ast, fn_name, args)
        {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                // If the function simply doesn't exist in the script, that's OK.
                if msg.contains("Function not found") {
                    Ok(())
                } else {
                    Err(ScriptError::Runtime(format!("{fn_name}: {msg}")))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> (ScriptEngine, mpsc::UnboundedReceiver<ScriptCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(KvmState::default()));
        (ScriptEngine::new(state, tx), rx)
    }

    #[test]
    fn compile_and_run_simple_script() {
        let (engine, _rx) = make_engine();
        let ast = engine.compile(r#"let x = 1 + 2;"#).unwrap();
        engine.run(&ast).unwrap();
    }

    #[test]
    fn log_sends_command() {
        let (engine, mut rx) = make_engine();
        let ast = engine.compile(r#"log("hello from rhai");"#).unwrap();
        engine.run(&ast).unwrap();

        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, ScriptCommand::Log(ref s) if s == "hello from rhai"));
    }

    #[test]
    fn missing_event_handler_is_ok() {
        let (engine, _rx) = make_engine();
        let ast = engine.compile(r#"let x = 42;"#).unwrap();
        let event = ScriptEvent::PeerConnected {
            peer_name: "test".into(),
        };
        engine.dispatch_event(&ast, &event).unwrap();
    }

    #[test]
    fn event_handler_receives_args() {
        let (engine, mut rx) = make_engine();
        let ast = engine
            .compile(
                r#"
                fn on_peer_connected(name) {
                    log("connected: " + name);
                }
            "#,
            )
            .unwrap();

        let event = ScriptEvent::PeerConnected {
            peer_name: "workstation".into(),
        };
        engine.dispatch_event(&ast, &event).unwrap();

        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, ScriptCommand::Log(ref s) if s.contains("workstation")));
    }

    #[test]
    fn switch_to_screen_command() {
        let (engine, mut rx) = make_engine();
        let ast = engine.compile(r#"switch_to_screen(2);"#).unwrap();
        engine.run(&ast).unwrap();

        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, ScriptCommand::SwitchToScreen(2)));
    }

    #[test]
    fn sandboxing_limits_operations() {
        let (engine, _rx) = make_engine();
        let ast = engine
            .compile(r#"let x = 0; loop { x += 1; }"#)
            .unwrap();
        assert!(engine.run(&ast).is_err());
    }
}
