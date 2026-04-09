# Dev

## Development Commands

```bash
cargo build                              # Build the project
cargo test                               # Run all tests
cargo test test_name                     # Run a single test
```

## Architecture

### Key Types
- `BR<T>` - Result alias for `Result<T, BbkarError>` (used throughout)
- `Timestamp` - A btrbk timestamp suffix (e.g. `20250101`, `20250101T1531_1`). Not the full snapshot name.
- `Series` - Snapshots sharing one source subvolume (basename + sorted list of `Timestamp`s)
- `DestState` / `DestMeta` - Existing archives at a destination (metadata about archived timestamps)
- `RunPlan` / `RunStep` - Backup plan: `SendFull` or `SendIncremental`
- `RestorePlan` / `RestoreStep` - Restore plan: `ReceiveFull` or `ReceiveIncremental`

### Module Layout

```
src/
├── main.rs          # CLI entry point (clap)
├── lib.rs           # Re-exports model types
├── cli/             # CLI Commands
├── model/           # pure value types
├── service/         # business logic
└── utils/           # shared helpers, formatting, logging
```
## Convention

### Logging

- ERROR: operation failed and cannot continue
- WARN: unexpected or degraded state, but command can continue
- INFO: normal user-visible status and high-level progress
- DEBUG: developer-oriented diagnostics
- TRACE: very detailed internal diagnostics
- use `tracing` for all runtime output; avoid direct `println!` / `eprintln!`
- INFO should render without a visible level prefix in normal terminal output
- INFO should only include a target when the event comes from outside `bbkar`
- WARN and ERROR should remain visually distinct in terminal output
- DEBUG and TRACE should include the emitting module target

Verbosity flags:

- default: `INFO`
- `-v`: `DEBUG`
- `-vv` and above: `TRACE`
