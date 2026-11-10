# Contributing

## Setup

```sh
mise install && mise run setup
```

See the [README](README.md#develop) for the system packages Linux needs.

## Checks

Run these before opening a pull request:

```sh
mise run lint
mise run test
mise run smoke
mise run audit
```

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org) and put the issue number last, for example
`fix(core): serialize project updates (#12)`.

For a user-visible change, add a line under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md).

## Releasing

1. Bump `version` under `[workspace.package]` in `Cargo.toml`.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [x.y.z] - YYYY-MM-DD` and add a new, empty `## [Unreleased]` above it.
3. Tag the commit `vx.y.z` and push the tag.

The release fails without a `## [x.y.z]` section in `CHANGELOG.md`; that section becomes the release notes.
