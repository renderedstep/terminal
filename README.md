# text-adventure-terminal

A terminal client for the text-adventure game engine. It speaks the engine's
protocol v1, revision 1.1 (HTTP with JSON, and Server-Sent Events for each turn), so it plays
against any engine that serves that protocol.

```
┌ The Causeway Court ─────────────────────────────┐┌ Room ──────────────────────────┐
│> /go The Causeway Court                         ││The Causeway Court              │
│                                                 │└────────────────────────────────┘
│You arrive at The Causeway Court. …              │┌ Ways out ──────────────────────┐
│» moved · toll: 2 damage                         ││The Tide Post                   │
│                                                 ││The Vestry Hulk                 │
└─────────────────────────────────────────────────┘└────────────────────────────────┘
┌ /go /talk /inspect /drop /attack /throw /use ───┐┌ Condition ─────────────────────┐
│> /inspect Ass                                  ││hurt (16 of 18)                 │
└─────────────────────────────────────────────────┘└────────────────────────────────┘
```

The right-hand panels come from the engine's glance: the room, ways out,
people here, things lying here, what you carry, your condition and the next
story beat. Prose streams into the log as it is written and is replaced by the
saved text when the turn finishes, followed by the turn's outcome and any dice
it rolled. Under 80 columns the panels fold into a short status block.

## Install

Releases are attached to this repository's GitHub Releases, which are private
for now, so installing uses your own [GitHub CLI](https://cli.github.com) login.
You need `gh` signed in (`gh auth login`) with access to this repository.

macOS and Linux (installs to `~/.local/bin`):

```sh
gh api repos/renderedstep/terminal/contents/install.sh \
  -H 'Accept: application/vnd.github.raw' | sh
```

Windows (PowerShell; installs to `%LOCALAPPDATA%\Programs\ta` and adds it to
your user `PATH`):

```powershell
gh api repos/renderedstep/terminal/contents/install.ps1 `
  -H 'Accept: application/vnd.github.raw' | Out-String | Invoke-Expression
```

Both scripts download the archive for your platform with `gh release
download`, check it against the release's `SHA256SUMS`, and refuse to install
on a mismatch. Set `TA_VERSION=v0.2.0` to install a particular release, or
`TA_INSTALL_DIR` to install somewhere else.

Builds are published for macOS (Apple silicon and Intel), Linux (x86_64 and
arm64) and Windows (x86_64).

From source, with a Rust toolchain: `cargo install --locked --path .`

## Upgrading

```sh
ta upgrade          # install the newest release in place of this binary
ta upgrade --check  # only say whether a newer release exists
```

`ta upgrade` uses your `gh` login too: it finds the latest release, downloads
the archive for the platform it was built for, verifies the checksum, and
replaces itself. Running the install script again does the same.

The repository moved from `calebl/text-adventure-terminal` to
`renderedstep/terminal`. GitHub redirects the old name, so a `ta` installed
before the move still finds and upgrades to new releases.

## Configuration

The client needs two things, both from the engine's operator: the engine's
address and your player token. There is no default address.

```sh
ta login --engine http://engine.example:8080   # saves the address, asks for the token
ta whoami                                      # checks both, shows this month's spend
```

Each setting is taken from the first place it is found:

| Setting | Flag | Environment | Stored |
| --- | --- | --- | --- |
| Engine address | `--engine URL` | `TA_ENGINE_URL` | `engine_url` in `config.toml` |
| Player token | `--token TOKEN` | `TA_TOKEN` | the system keychain, or a private file |

`config.toml` lives in the OS config directory, under
`text-adventure-terminal/` (`~/.config` on Linux, `~/Library/Application
Support` on macOS, `%APPDATA%` on Windows). `TA_CONFIG_DIR` points it
elsewhere. `ta config` shows which source each setting came from.

`ta login` keeps the token in the system keychain (macOS Keychain, Windows
Credential Manager, or the Secret Service on Linux). Where no keychain is
available, it falls back to a `token` file beside `config.toml` that only you
can read (mode 0600). `TA_NO_KEYRING=1` skips the keychain. The token is never
printed or logged; `ta logout` forgets it.

The address may be given with or without the `/api/v1` suffix.

### Reaching an engine over Tailscale

An engine that runs on someone's own machine is usually reached over their
Tailscale tailnet. Nothing in the client is specific to that: the engine
address is whatever the operator gives you, for example a MagicDNS name or a
`100.x.y.z` address with its port. Join the tailnet, check you can reach it
(`tailscale ping <host>`), then `ta login --engine http://<host>:<port>`.

## Playing

```sh
ta worlds            # the worlds a game can be started in
ta new <world>       # start a game
ta games             # your games
ta                   # the start screen: continue a game or start one
ta --resume          # continue the game you started last, skipping the start screen
ta play [game]       # the full-screen client (the default command)
ta line [game]       # plain line mode
```

`ta` (or `ta play` with no game) opens on a start screen, like the engine's
web page: your games to continue, unfinished ones first and the latest at the
top, each with its world and when it was started, then the worlds a new game
can be started in. Up and Down (or `j` and `k`) choose, Enter plays, Esc or
`q` quits. If the engine refuses a choice, its reason is shown and you can
choose again.

`ta --resume` (or `ta play --resume`) skips the start screen and continues
your unfinished game that was started last. With none to continue, it says so
on the start screen. The engine does not report when a game was last played,
so "last" means last started. `ta line` with no game continues the same game.

In the full-screen client:

- Type a line and press Enter. Lines starting with `/` use the engine's
  verbs, with the words the engine names for them; Tab completes the verb,
  then its target from the glance (`/ta` → `/take `, `/take fi` →
  `/take Filing press`). Tab again cycles.
- `use` targets are whole attempts, each played by a line the engine gives.
  `/use ` and Tab completes a target to its line (`/use off` →
  `/offer tide-slate to Ammon Brace`), and `/use <target>` sent as it is goes
  as that target's line too.
- PageUp and PageDown scroll the log.
- `:resume` finishes a turn the engine saved when it was interrupted, with
  the same request token so it is not played twice; `:ack` lets go of one
  that cannot be resumed. `:refresh` redraws from the engine.
- When a side panel is too short for its list, its last line says how many
  rows are hidden ("+2 more · :exits"). Ways out keeps its height before the
  other panels shrink. `:exits`, `:here`, `:items` and `:carrying` print a
  panel's full list into the log, and `:panels` prints them all. `:help`
  lists the client's commands.
- Esc or `/menu` goes back to the start screen, at any time and after a
  game ends; Ctrl-C or `:q` quits.

Refusals, the monthly spend limit, rate limits and a rejected token are
shown in the engine's own words. If the connection drops during a turn, the
client reconnects from the last event it received, so nothing is shown
twice. Opening a game while a turn is still playing follows that turn's
events to the end, in both the full-screen client and line mode.

### Line mode, for scripts and agents

`ta line` reads lines from stdin, plays each as a turn, and writes the saved
prose, the engine's notices (`* …`) and the outcome (`» …`) to stdout, with
no terminal control codes. Errors are written as `! …` lines. With `--json`,
each finished turn is written as one line of JSON (the protocol's `finished`
event), and each error as `{"error": {...}}`. The exit status is 2 when any
line was refused with an error, and it stops at a rejected token.

```sh
printf '/inspect Assize tide-slate\n/go The Tide Post\n' | ta line --json
```

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The engine's pure rules (dice and geometry) in Rust now live in their own
repository, [renderedstep/engine](https://github.com/renderedstep/engine), as
the `renderedstep-engine` crate. The client does not use it yet.

- `src/engine/` holds the `Engine` trait, the one seam between the interface
  and a game. `http.rs` implements protocol v1; `fake.rs` is an in-memory
  engine for tests. An in-process engine would implement the same trait.
- `src/protocol.rs` has the protocol's types. `spec/openapi.json` is a copy of
  the engine's published contract, and `tests/contract.rs` checks the types
  against it. The tests use the contract's own examples as payloads
  (`tests/common/`), the same ones the engine's tests hold to its real
  responses. When the protocol grows, update that copy and the tests say
  what changed.
- `tests/tui.rs` renders screens into a test terminal and compares them with
  the snapshots in `tests/snapshots/` (review changes with `cargo insta
  review`, or `INSTA_UPDATE=always cargo test`).
- `tests/http.rs` runs the HTTP engine against a scripted local server,
  including a stream dropped mid-event and resumed with `Last-Event-ID`.

### Releasing

Bump `version` in `Cargo.toml`, merge, then push a matching tag:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

`.github/workflows/release.yml` builds every target and attaches the archives
and `SHA256SUMS` to a GitHub Release. The tag must match the crate version.

`dist-workspace.toml` holds a [cargo-dist](https://opensource.axo.dev/cargo-dist/)
configuration for when the repository goes public. Its public installers are
switched off and it generates no CI today.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
