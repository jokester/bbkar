# milestones

## v0.1.0: MVP (CURRENT)

- [x] code builds and runs
- [x] stable metadata, config format, backup file conventions
- [x] compression
- [x] support for local / S3 / GCS storage
- [ ] configurable retention
- [ ] retention UX
  - [ ] `status` explains the retention plan
  - [ ] `ls` shows whether each archive would be pruned on the next `run`
- commands:
  - [ ] `status`
  - [ ] `ls`
  - [x] `run`
  - [x] `dryrun`
  - [x] `restore`
  - [ ] `prune`

- [ ] keep sudo session alive during long runs without re-prompting

## post-v0.1.0 (NOT NOW)

- [ ] encryption
- [ ] `bbkar scrub`
- [ ] rewrite with async
    - [ ] concurrency IO
