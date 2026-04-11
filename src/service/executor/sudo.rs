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
        Self::new_with(
            determine_execution_strategy,
            ensure_sudo_authenticated,
            Instant::now,
        )
    }

    fn new_with(
        determine_strategy: impl FnOnce() -> BR<ExecutionStrategy>,
        authenticate: impl FnOnce() -> BR<()>,
        now: impl FnOnce() -> Instant,
    ) -> BR<Self> {
        let strategy = determine_strategy()?;

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
                authenticate()?;
                info!("Authentication successful. Proceeding with backup...");
            }
        }

        Ok(Self {
            last_refresh: now(),
            refresh_interval: Duration::from_secs(10 * 60), // 10 minutes
            strategy,
        })
    }

    pub fn needs_sudo(&self) -> bool {
        !matches!(self.strategy, ExecutionStrategy::Direct)
    }

    pub fn ensure_active(&mut self) -> BR<()> {
        self.ensure_active_with(run_sudo_refresh, Instant::now)
    }

    fn ensure_active_with(
        &mut self,
        refresh: impl FnOnce() -> BR<()>,
        now: impl FnOnce() -> Instant,
    ) -> BR<()> {
        match self.strategy {
            ExecutionStrategy::Direct | ExecutionStrategy::SudoPasswordless => Ok(()),
            ExecutionStrategy::SudoInteractive => {
                if self.last_refresh.elapsed() > self.refresh_interval {
                    refresh()?;
                    self.last_refresh = now();
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
impl SudoSession {
    pub(crate) fn test_session(strategy: ExecutionStrategy) -> Self {
        Self {
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(1),
            strategy,
        }
    }
}

/// Check if the current process is running as root
pub fn is_running_as_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Determine the execution strategy based on current privileges and sudo configuration
pub fn determine_execution_strategy() -> BR<ExecutionStrategy> {
    determine_execution_strategy_with(is_running_as_root, is_interactive_stdin, sudo_check_status)
}

fn determine_execution_strategy_with(
    is_root: impl FnOnce() -> bool,
    is_interactive: impl FnOnce() -> bool,
    sudo_check: impl FnOnce() -> BR<bool>,
) -> BR<ExecutionStrategy> {
    if is_root() {
        return Ok(ExecutionStrategy::Direct);
    }

    // Check if sudo works without password
    if sudo_check()? {
        return Ok(ExecutionStrategy::SudoPasswordless);
    }

    // Check if we're in an interactive environment
    if is_interactive() {
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

fn sudo_check_status() -> BR<bool> {
    let status = Command::new("sudo")
        .args(["-n", "id"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| BbkarError::Execution(format!("Failed to check sudo access: {}", e)))?;

    Ok(status.success())
}

fn is_interactive_stdin() -> bool {
    atty::is(atty::Stream::Stdin)
}

/// Ensure sudo is authenticated by running sudo -v
pub fn ensure_sudo_authenticated() -> BR<()> {
    ensure_sudo_authenticated_with(run_sudo_authenticate)
}

fn ensure_sudo_authenticated_with(authenticate: impl FnOnce() -> BR<()>) -> BR<()> {
    authenticate()
}

fn run_sudo_authenticate() -> BR<()> {
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

fn run_sudo_refresh() -> BR<()> {
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .map_err(|e| BbkarError::Execution(format!("Failed to refresh sudo: {}", e)))?;

    if !status.success() {
        return Err(BbkarError::Execution(
            "Failed to refresh sudo credentials".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_execution_strategy_direct_when_running_as_root() {
        let strategy = determine_execution_strategy_with(|| true, || false, || Ok(false)).unwrap();
        assert!(matches!(strategy, ExecutionStrategy::Direct));
    }

    #[test]
    fn test_determine_execution_strategy_passwordless_when_sudo_nopasswd_works() {
        let strategy = determine_execution_strategy_with(|| false, || false, || Ok(true)).unwrap();
        assert!(matches!(strategy, ExecutionStrategy::SudoPasswordless));
    }

    #[test]
    fn test_determine_execution_strategy_interactive_when_terminal_is_available() {
        let strategy = determine_execution_strategy_with(|| false, || true, || Ok(false)).unwrap();
        assert!(matches!(strategy, ExecutionStrategy::SudoInteractive));
    }

    #[test]
    fn test_determine_execution_strategy_errors_when_non_interactive_and_no_sudo() {
        let err =
            determine_execution_strategy_with(|| false, || false, || Ok(false)).unwrap_err();
        let rendered = format!("{err}");
        assert!(rendered.contains("Cannot execute btrfs commands"));
        assert!(rendered.contains("Configure passwordless sudo"));
    }

    #[test]
    fn test_ensure_sudo_authenticated_propagates_failure() {
        let err = ensure_sudo_authenticated_with(|| {
            Err(BbkarError::Execution("Sudo authentication failed".to_string()))
        })
        .unwrap_err();
        assert!(format!("{err}").contains("Sudo authentication failed"));
    }

    #[test]
    fn test_new_with_authenticates_interactive_sessions() {
        let mut authenticated = false;
        let session = SudoSession::new_with(
            || Ok(ExecutionStrategy::SudoInteractive),
            || {
                authenticated = true;
                Ok(())
            },
            Instant::now,
        )
        .unwrap();

        assert!(authenticated);
        assert!(matches!(session.strategy, ExecutionStrategy::SudoInteractive));
    }

    #[test]
    fn test_needs_sudo_only_for_non_direct_strategy() {
        let direct = SudoSession {
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::Direct,
        };
        let sudo = SudoSession {
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::SudoInteractive,
        };

        assert!(!direct.needs_sudo());
        assert!(sudo.needs_sudo());
    }

    #[test]
    fn test_ensure_active_skips_refresh_for_direct_and_passwordless() {
        let mut direct = SudoSession {
            last_refresh: Instant::now() - Duration::from_secs(3600),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::Direct,
        };
        let mut passwordless = SudoSession {
            last_refresh: Instant::now() - Duration::from_secs(3600),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::SudoPasswordless,
        };

        assert!(direct.ensure_active_with(|| Err(BbkarError::Execution("no".into())), Instant::now).is_ok());
        assert!(passwordless
            .ensure_active_with(|| Err(BbkarError::Execution("no".into())), Instant::now)
            .is_ok());
    }

    #[test]
    fn test_ensure_active_refreshes_interactive_session_after_interval() {
        let start = Instant::now();
        let refreshed_at = start + Duration::from_secs(5);
        let mut refreshed = false;
        let mut session = SudoSession {
            last_refresh: start - Duration::from_secs(1200),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::SudoInteractive,
        };

        session
            .ensure_active_with(
                || {
                    refreshed = true;
                    Ok(())
                },
                || refreshed_at,
            )
            .unwrap();

        assert!(refreshed);
        assert_eq!(session.last_refresh, refreshed_at);
    }

    #[test]
    fn test_ensure_active_propagates_refresh_failure() {
        let mut session = SudoSession {
            last_refresh: Instant::now() - Duration::from_secs(1200),
            refresh_interval: Duration::from_secs(1),
            strategy: ExecutionStrategy::SudoInteractive,
        };

        let err = session
            .ensure_active_with(
                || Err(BbkarError::Execution("Failed to refresh sudo credentials".into())),
                Instant::now,
            )
            .unwrap_err();

        assert!(format!("{err}").contains("Failed to refresh sudo credentials"));
    }
}
