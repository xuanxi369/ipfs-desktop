# IPFS Desktop Rust

[简体中文](README.md) | [English](README_EN.md)

An IPFS desktop client built with Tauri 2, Rust, React, and TypeScript. It provides a desktop interface for managing Kubo, files, pins, IPNS records, node status, and network connections.

## Architecture

The node backend is [Kubo](https://github.com/ipfs/kubo), the Go implementation of IPFS. Rust and Tauri manage the desktop window, daemon lifecycle, HTTP RPC access, caching, offline operations, and the system tray. React provides the user interface.

The repository also contains extension points for a native iroh backend and dual-backend routing. Kubo is the default for compatibility with the existing IPFS network and CIDs. The iroh functionality is experimental and is not a complete Kubo replacement.

## Features

- Start, stop, restart, and monitor the Kubo daemon
- Dashboard for node status, peers, bandwidth, Bitswap, repository data, and health
- Approximate geographic map of publicly locatable peers
- File upload, CID preview, streaming download, and download progress
- List, add, and remove pins
- Publish and resolve IPNS records and manage Kubo key labels
- Embedded IPFS Web UI
- Cache, offline operation queue, automatic replay, and background health checks
- System tray, launch at login, and automatic restart after a Kubo crash
- Advanced tools for API/Gateway configuration, MFS debugging, binary information, and routing policy
- Chinese and English user interfaces
- Optional experimental iroh file transfer, BlobTicket, keep/unkeep, and dual-backend routing

## Build Variants

| Capability | Default build (with `iroh-backend`) | `--no-default-features` compatibility build |
| --- | --- | --- |
| Kubo daemon and IPFS RPC | Available on demand as a compatibility bridge | Available as the only backend |
| Files, pins, IPNS, MFS, Web UI | Available | Available through Kubo |
| Cache, offline queue, bandwidth, health checks | Available | Available |
| iroh identity and version information | Native implementation | Stub |
| iroh add/cat, BlobTicket, keep/unkeep | Native implementation | Unsupported |
| Automatic Kubo/iroh routing | Auto by default; LocalFirst/Compatible/Mirrored available | Kubo compatibility path only |
| Recommended audience | Development and validation | Kubo-only compatibility and troubleshooting |

The default build enables the real iroh backend and keeps Kubo as the IPFS/IPNS/Gateway compatibility bridge. For a Kubo-only compatibility build:

```bash
npm run tauri dev -- --no-default-features
```

## Requirements

- Node.js 18 or later and npm
- Stable Rust toolchain for Tauri development and packaging
- Tauri 2 platform dependencies
  - macOS: Xcode Command Line Tools
  - Windows: Microsoft C++ Build Tools and WebView2
  - Linux: WebKitGTK, a compiler, and window-system development packages; see [Tauri prerequisites](https://tauri.app/start/prerequisites/)

The app resolves the Kubo executable in this order:

1. The executable specified by `IPFS_GO_EXEC`
2. `ipfs` (`ipfs.exe` on Windows) in the system `PATH`
3. A bundled executable in the application directory, `bin/`, or `resources/`

An external Kubo installation can be initialized with:

```bash
ipfs init
```

## Quick Start

```bash
npm install
npm run setup:kubo
npm run tauri dev
```

On Windows, `npm run setup:kubo` downloads a stable Kubo binary from the official distribution site, verifies the official SHA-512 sidecar, and places it at `src-tauri/resources/ipfs.exe` for Tauri packaging. If the repository has not been initialized when the node starts, the app automatically performs the equivalent of `ipfs init`; regular users do not need to install Kubo or open a terminal.

Advanced users can override the bundled binary with `IPFS_GO_EXEC` or the system `PATH`. The default Kubo RPC endpoint is `http://127.0.0.1:5001`, and the default gateway is `http://127.0.0.1:8080`.

To run only the frontend:

```bash
npm run dev
```

This mode does not start the Rust/Tauri backend, so daemon-related controls will not work.

## Validation

```bash
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings -A deprecated
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

The generated `dist/` directory is reproducible and should not be committed.

## Kubo Troubleshooting

If the daemon cannot start, first check for an old `ipfs` or `kubo` process occupying the ports.

Windows PowerShell:

```powershell
Get-Process ipfs,kubo -ErrorAction SilentlyContinue
Stop-Process -Name ipfs,kubo -Force -ErrorAction SilentlyContinue
```

macOS/Linux:

```bash
pgrep -af '(^|/)(ipfs|kubo)( |$)'
pkill -TERM -f '(^|/)(ipfs|kubo)( |$)'
```

You can also inspect the API/Gateway addresses in Advanced Tools or explicitly select a Kubo binary:

```bash
IPFS_GO_EXEC=/path/to/ipfs npm run tauri dev
```

Application logs are stored under:

- macOS: `~/Library/Application Support/ipfs-desktop-rust/logs/`
- Linux: `~/.local/share/ipfs-desktop-rust/logs/`
- Windows: `%LOCALAPPDATA%\ipfs-desktop-rust\logs\`

## Application Data

| Data | File |
| --- | --- |
| Configuration | `config.json` |
| SQLite cache | `cache.db` |
| Offline queue | `offline_queue.db` |
| Public IPNS label records | `keys/*.json` |
| Logs | `logs/app.log` |

IPNS private keys are managed by the Kubo keystore. This application does not generate, persist, or transmit those private keys over IPC.

## Project Structure

```text
src/                            React frontend, components, styles, and i18n
src-tauri/src/                  Rust/Tauri backend
src-tauri/src/commands.rs       Tauri command registration and shared command logic
src-tauri/src/commands_binary.rs  Binary-related commands
src-tauri/src/daemon/           Kubo discovery, process control, and RPC client
src-tauri/src/backend_router.rs Kubo/iroh routing policy
docs/                           Roadmap, phase notes, benchmarks, and release checklist
COMMANDS.md                     Tauri command index
```

## Security and Privacy

- The peer map handles only available public addresses and shows approximate regions. Private, relay, DNS, and anonymized addresses are filtered or marked unknown.
- Geolocation is inferred from IP addresses and does not represent a node's exact location.
- The Kubo API connects to localhost by default. Review access controls and firewall rules before changing the API address.
- Never commit application data, logs, the Kubo repository, or keystore material.

## Documentation

- [Tauri command index](COMMANDS.md)
- [Project documentation index](docs/README.md)
- [Project roadmap](docs/PROJECT_ROADMAP.md)
- [Contributing guide](CONTRIBUTING.md)
- [Release checklist](docs/RELEASE_CHECKLIST.md)

## Contributors

<a href="https://github.com/xuanxi369/IPFS-Desktop-Rust/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=xuanxi369/IPFS-Desktop-Rust" alt="Project contributors" />
</a>

## License

This project is dual-licensed under your choice of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

SPDX identifier: `MIT OR Apache-2.0`. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project is dual-licensed as above, without additional terms or conditions.
