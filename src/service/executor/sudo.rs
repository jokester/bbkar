use crate::model::error::{BR, BbkarError};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::info;

/// Execution strategy based on current privileges
#[derive(Debug, Clone)]
pub enum ExecutionStrategy {
    Direct,           // Running as root
    SudoPasswordless, // sudo without password
    SudoInteractive,  // sudo with password prompt
}

/// Manages sudo session to avoid repeated password prompts
pub struct SudoSession {
    last_refresh: Instant,
    refresh_interval: Duration,
    pub strategy: ExecutionStrategy,
}

impl SudoSession {
    pub fn new() -> BR<Self> {
        let strategy = determine_execution_strategy()?;

        // Authenticate once at the beginning if needed
        match strategy {
            ExecutionStrategy::Direct => {
                info!("Running as root - no sudo required");
            }
            ExecutionStrategy::SudoPasswordless => {
                info!("Using passwordless sudo for btrfs commands");
            }
            ExecutionStrategy::SudoInteractive => {
                info!("Authenticating for sudo access...");
                ensure_sudo_authenticated()?;
                info!("Authentication successful. Proceeding with backup...");
            }
        }

        Ok(Self {
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(10 * 60), // 10 minutes
            strategy,
        })
    }

    pub fn needs_sudo(&self) -> bool {
        !matches!(self.strategy, ExecutionStrategy::Direct)
    }

    pub fn ensure_active(&mut self) -> BR<()> {
        match self.strategy {
            ExecutionStrategy::Direct | ExecutionStrategy::SudoPasswordless => Ok(()),
            ExecutionStrategy::SudoInteractive => {
                if self.last_refresh.elapsed() > self.refresh_interval {
                    let status = Command::new("sudo").arg("-v").status().map_err(|e| {
                        BbkarError::Execution(format!("Failed to refresh sudo: {}", e))
                    })?;

                    if !status.success() {
                        return Err(BbkarError::Execution(
                            "Failed to refresh sudo credentials".to_string(),
                        ));
                    }

                    self.last_refresh = Instant::now();
                }
                Ok(())
            }
        }
    }
}

/// Check if the current process is running as root
pub fn is_running_as_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Determine the execution strategy based on current privileges and sudo configuration
pub fn determine_execution_strategy() -> BR<ExecutionStrategy> {
    if is_running_as_root() {
        return Ok(ExecutionStrategy::Direct);
    }

    // Check if sudo works without password
    let status = Command::new("sudo")
        .args(["-n", "id"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| BbkarError::Execution(format!("Failed to check sudo access: {}", e)))?;

    if status.success() {
        return Ok(ExecutionStrategy::SudoPasswordless);
    }

    // Check if we're in an interactive environment
    if atty::is(atty::Stream::Stdin) {
        return Ok(ExecutionStrategy::SudoInteractive);
    }

    Err(BbkarError::Execution(
        "Cannot execute btrfs commands. Options:\n\
         1. Run as root\n\
         2. Configure passwordless sudo for btrfs commands\n\
         3. Run in an interactive terminal"
            .to_string(),
    ))
}

/// Ensure sudo is authenticated by running sudo -v
pub fn ensure_sudo_authenticated() -> BR<()> {
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .map_err(|e| BbkarError::Execution(format!("Failed to authenticate sudo: {}", e)))?;

    if !status.success() {
        return Err(BbkarError::Execution(
            "Sudo authentication failed".to_string(),
        ));
    }

    Ok(())
}
