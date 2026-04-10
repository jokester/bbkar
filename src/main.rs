use bbkar::cli;
use bbkar::model::error::BR;
use bbkar::service::executor::summon_real_executor;
use bbkar::utils::logging::init_tracing;
use clap::Parser;
use tracing::error;

use bbkar::cli::Cli;

fn count_config_args(args: impl IntoIterator<Item = String>) -> usize {
    args.into_iter()
        .filter(|a| {
            a == "--config"
                || a == "-c"
                || a.starts_with("--config=")
                || (a.starts_with("-c") && a.len() > 2 && !a.starts_with("--"))
        })
        .count()
}

fn main() -> BR<()> {
    let config_count = count_config_args(std::env::args());
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let config_path = match config_count {
        0 => {
            error!("--config <CONFIG_FILE> is required");
            std::process::exit(2);
        }
        1 => cli.config.unwrap(),
        _ => {
            error!("--config specified more than once");
            std::process::exit(2);
        }
    };

    let executor = Box::new(summon_real_executor()?);
    let result = match cli.command {
        cmd @ cli::Commands::Dryrun { .. } => cli::dryrun(&config_path, cmd, executor),
        cli::Commands::Ls { name, restore_root } => cli::ls(
            &config_path,
            name.as_deref(),
            restore_root.as_deref(),
            executor,
        ),
        cli::Commands::Run { name } => cli::run(&config_path, name.as_deref(), executor),
        cli::Commands::Status { name } => cli::status(&config_path, name.as_deref(), executor),
        cli::Commands::Restore {
            root,
            volume,
            snapshot,
            min_timestamp,
            max_timestamp,
            name,
        } => cli::restore(
            &config_path,
            &root,
            &volume,
            &snapshot,
            min_timestamp.as_deref(),
            max_timestamp.as_deref(),
            name.as_deref(),
            executor,
        ),
        cli::Commands::DryRestore {
            root,
            volume,
            snapshot,
            min_timestamp,
            max_timestamp,
            name,
        } => cli::dryrestore(
            &config_path,
            &root,
            &volume,
            &snapshot,
            min_timestamp.as_deref(),
            max_timestamp.as_deref(),
            name.as_deref(),
            executor,
        ),
    };

    if let Err(e) = result {
        error!("{}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::count_config_args;

    #[test]
    fn test_count_config_args_variants() {
        assert_eq!(
            count_config_args(vec!["bbkar".to_string(), "--config".to_string(), "a.toml".to_string()]),
            1
        );
        assert_eq!(
            count_config_args(vec!["bbkar".to_string(), "--config=a.toml".to_string()]),
            1
        );
        assert_eq!(
            count_config_args(vec!["bbkar".to_string(), "-c".to_string(), "a.toml".to_string()]),
            1
        );
        assert_eq!(
            count_config_args(vec!["bbkar".to_string(), "-ca.toml".to_string()]),
            1
        );
    }

    #[test]
    fn test_count_config_args_ignores_non_config_args() {
        assert_eq!(
            count_config_args(vec![
                "bbkar".to_string(),
                "--check".to_string(),
                "--color=always".to_string(),
                "-v".to_string(),
            ]),
            0
        );
        assert_eq!(
            count_config_args(vec![
                "bbkar".to_string(),
                "--config=a.toml".to_string(),
                "-cb.toml".to_string(),
            ]),
            2
        );
    }
}
