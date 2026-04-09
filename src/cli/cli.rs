use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the configuration file
    #[arg(short, long, global = true, value_name = "CONFIG_FILE")]
    pub config: Option<PathBuf>,

    /// Increase log verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List local subvolumes / remote archives
    Status {
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
    },
    /// List snapshot/archive names and whether they are only local or only remote
    Ls {
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
        /// Path to a restore root; shows which archives have been restored
        #[arg(long)]
        restore_root: Option<String>,
    },
    /// Export subvolumes and sync to backup locations
    Run {
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
    },
    /// Like run, but only prints what would be done
    Dryrun {
        /// Actually run btrfs send and measure compressed size
        #[arg(long, default_value_t = false)]
        measure_send: bool,
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
    },
    /// Restore snapshots from archive via btrfs receive
    Restore {
        /// Path to the local btrfs volume to receive into
        #[arg(long)]
        root: String,
        /// Volume name to restore (required)
        #[arg(long)]
        volume: String,
        /// Specific snapshot(s) to restore (repeatable; default: latest)
        #[arg(long)]
        snapshot: Vec<String>,
        /// Only restore snapshots at or after this timestamp
        #[arg(long)]
        min_timestamp: Option<String>,
        /// Only restore snapshots at or before this timestamp
        #[arg(long)]
        max_timestamp: Option<String>,
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
    },
    /// Like restore, but only prints what would be done
    DryRestore {
        /// Path to the local btrfs volume to receive into
        #[arg(long)]
        root: String,
        /// Volume name to restore (required)
        #[arg(long)]
        volume: String,
        /// Specific snapshot(s) to restore (repeatable; default: latest)
        #[arg(long)]
        snapshot: Vec<String>,
        /// Only restore snapshots at or after this timestamp
        #[arg(long)]
        min_timestamp: Option<String>,
        /// Only restore snapshots at or before this timestamp
        #[arg(long)]
        max_timestamp: Option<String>,
        /// Name of the [sync] block to use
        #[arg(long)]
        name: Option<String>,
    },
}
