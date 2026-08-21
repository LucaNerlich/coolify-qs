# coolify-qs

[![GitHub Release](https://img.shields.io/github/v/release/LucaNerlich/coolify-qs)](https://github.com/LucaNerlich/coolify-qs/releases)

Omarchy Quattro bar widget showing deployment activity across your
[Coolify](https://coolify.io/) servers: the bar shows how many deployments are
running (`⟳ 1`) and queued (`⏳ 2`), and the panel lists current and recent
deployments grouped by server and application.

## Requirements

- Omarchy Quattro (Quickshell-based shell)
- One or more self-hosted Coolify instances (v4.0.0-beta or later) with the
  API enabled and an API token
- `~/.config/coolify-qs/config.json` — see [Configuration](#configuration)

## Architecture

- **Rust backend** (`coolify-qs` binary): polls each server's
  `GET /api/v1/applications`, then
  `GET /api/v1/deployments/applications/{uuid}` per application, aggregates
  the results, and streams one JSON snapshot per change.
- **QML frontend** (`omarchy/`): a `bar-widget` plugin. `BarWidget.qml` runs
  `coolify-qs watch` once and updates from its JSON lines; `Panel.qml` shows
  the per-server, per-application deployment list. All API traffic stays in
  Rust; the QML is pure presentation.

```
coolify-qs watch ──(JSON lines)──▶ SplitParser ─▶ BarWidget ─▶ Panel
```

## Configuration

### Getting started

1. **Create an API token** in each Coolify instance: open its UI, go to
   **Keys & Tokens → API tokens**, and create a new token. The token needs
   read access to the applications you want to watch.

2. **Create the config file** from the bundled example:

   ```bash
   mkdir -p ~/.config/coolify-qs
   cp config.example.json ~/.config/coolify-qs/config.json
   chmod 600 ~/.config/coolify-qs/config.json
   ```

3. **Fill in your servers.** Replace the example entries with your own
   `url` and `token` values (and set a `name` you like — it defaults to the
   URL host). Add or remove `servers` entries freely; one entry per Coolify
   instance.

   The widget re-reads the file on every poll, so changes apply within a
   poll interval — no shell restart needed. If the file is missing or
   invalid the bar shows `🚀 !` with the reason in the panel.

### Format

```json
{
  "pollIntervalSeconds": 15,
  "pastPerApp": 5,
  "servers": [
    {
      "name": "home",
      "url": "https://coolify.example.com",
      "token": "YOUR_API_TOKEN"
    }
  ]
}
```

A ready-to-edit example is included in the repository as
[`config.example.json`](config.example.json).

| Key | Default | Description |
| --- | --- | --- |
| `pollIntervalSeconds` | 15 | Poll interval (clamped to 5–3600). |
| `pastPerApp` | 5 | Number of recent deployments to fetch per application (1–100). |
| `notifications` | true | Send a desktop notification when a deployment finishes or fails. |
| `servers` | required | One entry per Coolify instance. |
| `servers[].name` | host | Display name; defaults to the URL host. |
| `servers[].url` | required | Coolify instance URL, `https://` or `http://`. |
| `servers[].token` | required | API token (Coolify → Keys & Tokens → API tokens). |

The `COOLIFY_QS_CONFIG` environment variable overrides the path entirely;
otherwise the backend reads `$XDG_CONFIG_HOME/coolify-qs/config.json`
(defaulting to `~/.config/coolify-qs/config.json`). Tokens never leave the
process — the watch stream contains no secrets. Keep the file readable only
by your user (`chmod 600`), it holds your API tokens.

## Install

```bash
omarchy plugin add https://github.com/LucaNerlich/coolify-qs.git --enable
```

Update / remove:

```bash
omarchy plugin update luca.coolify
omarchy plugin remove luca.coolify
```

The plugin bundles a statically linked x86_64 musl build of its backend
(`omarchy/bin/coolify-qs`), byte-for-byte reproducible from the tracked Rust
source (`make verify-bundle`, CI-gated). If the bundled binary cannot start —
non-x86_64 machine, missing exec bit, whatever — the widget falls back to a
`coolify-qs` binary on PATH (`cargo install coolify-qs`).

## Usage

- **Bar**: shows the running application's name (`⟳ website`, up to two
  names plus an overflow count) and the queued count (`⏳ 2`), or a plain
  `🚀` when idle. Turns urgent (`🚀 !`) on config errors. Left- or
  right-click opens the panel.
- **Panel**:
  - One column per server. Each application lists its current and recent
    deployments: status glyph (`⟳` running, `⏳` queued, `✓` finished,
    `✗` failed, `⊘` cancelled), commit message, short commit sha, and
    relative time. Long messages wrap inside their column; apps without
    deployment history collapse into a muted count caption.
  - Click a deployment row to open it in the Coolify UI; click a server
    header's host line to open the server.
  - Offline servers show their error inline.
- **Notifications**: when a deployment that was running or queued settles,
  a desktop notification appears through the Omarchy notification service:
  `✓ website deployed` on success, `✗ website deployment failed` on failure
  (with the commit message and server). Disable with
  `"notifications": false` in the config.
- **Shell**: `omarchy-shell shell summon luca.coolify '{}'` opens the panel,
  `omarchy-shell shell hide luca.coolify` closes it.

## Settings

Widget settings live in `~/.config/omarchy/shell.json`:

```bash
omarchy bar set luca.coolify hideWhenIdle true
```

| Key | Default | Description |
| --- | --- | --- |
| `hideWhenIdle` | false | Hide the widget entirely when no deployment is running or queued. |

## CLI

```bash
coolify-qs status        # one status snapshot as a single JSON line
coolify-qs watch         # stream snapshots, one per change
coolify-qs open --url X  # open an http(s) URL in the browser
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
node omarchy/model.test.mjs
omarchy plugin validate .
qmllint -I "$OMARCHY_PATH/shell" omarchy/BarWidget.qml omarchy/Panel.qml
```

`make bundle` rebuilds the statically linked backend into `omarchy/bin/` and
`make verify-bundle` checks that the committed binary is byte-identical to a
fresh reproducible build (both need the `x86_64-unknown-linux-musl` target
and a musl C cross-compiler — `pacman -S musl` on Arch, `musl-tools` on
Ubuntu, or the [musl.cc](https://musl.cc) toolchain on PATH; the ring TLS
backend compiles C sources). The toolchain is pinned in
`rust-toolchain.toml`, and CI verifies the bundle instead of regenerating it.

Saving files under an installed user plugin triggers Quattro's plugin hot
reload. Rerun `omarchy plugin validate .` after changing the manifest or
entry points. Note: on quickshell-git 0.3.0 `Qt.clearComponentCache` is
unavailable, so plugin QML changes only take effect after a full
`omarchy restart shell`.

## API notes

The Coolify reference docs mis-describe the deployments response as a bare
array of application objects (upstream issue #5874). The real endpoint
returns `{"count": n, "deployments": [...]}`. The backend accepts both
shapes and tolerates unknown status values.

## License

Apache-2.0. This project is not affiliated with Coolify; "Coolify" is their
trademark.
