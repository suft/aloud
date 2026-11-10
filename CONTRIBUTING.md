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

### Signing the macOS app

Release builds sign and notarize the macOS app when all six repository secrets below exist. With none of them
the app is unsigned; with only some, the release fails.

- `APPLE_CERTIFICATE`: the Developer ID Application certificate and its key, exported as `.p12` and
  base64-encoded (`openssl base64 -A -in certificate.p12`).
- `APPLE_CERTIFICATE_PASSWORD`: the `.p12` export password.
- `APPLE_SIGNING_IDENTITY`: for example `Developer ID Application: Your Name (TEAMID)`.
- `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID`: for notarization.

A signed build is checked with `codesign`, `spctl` and `stapler`. Once releases are signed, remove the Gatekeeper
steps from the install notes in `README.md`.
