//! Script manager: loading, hot-reload, and event dispatch.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rhai::AST;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::engine::{ScriptEngine, ScriptEvent};

/// A loaded and compiled script.
struct LoadedScript {
    path: PathBuf,
    ast: AST,
    last_modified: SystemTime,
}

/// Manages the lifecycle of Rhai scripts for S-KVM.
///
/// Responsibilities:
/// - Load `.rhai` scripts from a config directory
/// - Hot-reload: poll for file changes every 2 seconds
/// - Dispatch [`ScriptEvent`]s to all loaded scripts
pub struct ScriptManager {
    engine: ScriptEngine,
    scripts: Vec<LoadedScript>,
    script_dir: PathBuf,
    event_rx: mpsc::UnboundedReceiver<ScriptEvent>,
}

impl ScriptManager {
    /// Create a new script manager.
    ///
    /// * `engine`     – the configured [`ScriptEngine`]
    /// * `script_dir` – directory to scan for `.rhai` scripts
    /// * `event_rx`   – channel to receive KVM events
    pub fn new(
        engine: ScriptEngine,
        script_dir: PathBuf,
        event_rx: mpsc::UnboundedReceiver<ScriptEvent>,
    ) -> Self {
        Self {
            engine,
            scripts: Vec::new(),
            script_dir,
            event_rx,
        }
    }

    /// Run the manager event loop (consumes self).
    ///
    /// This loads all scripts on startup, then enters a select loop that:
    /// 1. Dispatches incoming events to all loaded scripts.
    /// 2. Periodically checks for new or modified scripts.
    pub async fn run(mut self) {
        // Initial load
        self.load_all_scripts().await;

        let mut reload_interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                event = self.event_rx.recv() => {
                    match event {
                        Some(ev) => self.dispatch_event(&ev),
                        None => {
                            info!("Script event channel closed, shutting down manager");
                            break;
                        }
                    }
                }
                _ = reload_interval.tick() => {
                    self.check_for_changes().await;
                }
            }
        }
    }

    // ── Loading ─────────────────────────────────────────────────────

    async fn load_all_scripts(&mut self) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.script_dir).await else {
            warn!(
                "Script directory not found: {}",
                self.script_dir.display()
            );
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rhai") {
                self.load_script(&path).await;
            }
        }

        info!(
            "Loaded {} script(s) from {}",
            self.scripts.len(),
            self.script_dir.display()
        );
    }

    async fn load_script(&mut self, path: &Path) {
        let source = match tokio::fs::read_to_string(path).await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read script {}: {e}", path.display());
                return;
            }
        };

        let ast = match self.engine.compile(&source) {
            Ok(ast) => ast,
            Err(e) => {
                warn!("Failed to compile script {}: {e}", path.display());
                return;
            }
        };

        let last_modified = file_modified(path).await;

        // Run top-level code (variable init, one-time setup)
        if let Err(e) = self.engine.run(&ast) {
            warn!(
                "Script initialisation error in {}: {e}",
                path.display()
            );
            return;
        }

        info!("Loaded script: {}", path.display());
        self.scripts.push(LoadedScript {
            path: path.to_owned(),
            ast,
            last_modified,
        });
    }

    // ── Hot-reload ──────────────────────────────────────────────────

    async fn check_for_changes(&mut self) {
        // Re-compile modified scripts
        for script in &mut self.scripts {
            let modified = file_modified(&script.path).await;
            if modified > script.last_modified {
                info!("Reloading modified script: {}", script.path.display());
                if let Ok(source) = tokio::fs::read_to_string(&script.path).await {
                    match self.engine.compile(&source) {
                        Ok(ast) => {
                            script.ast = ast;
                            script.last_modified = modified;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to recompile {}: {e}",
                                script.path.display()
                            );
                        }
                    }
                }
            }
        }

        // Discover newly added scripts
        let Ok(mut entries) = tokio::fs::read_dir(&self.script_dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rhai")
                && !self.scripts.iter().any(|s| s.path == path)
            {
                self.load_script(&path).await;
            }
        }
    }

    // ── Event dispatch ──────────────────────────────────────────────

    fn dispatch_event(&self, event: &ScriptEvent) {
        for script in &self.scripts {
            if let Err(e) = self.engine.dispatch_event(&script.ast, event) {
                error!(
                    "Script error in {} handling event: {e}",
                    script.path.display()
                );
            }
        }
    }
}

/// Helper: get the modification time of a file (or UNIX_EPOCH on error).
async fn file_modified(path: &Path) -> SystemTime {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}
