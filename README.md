# Cabinet Desktop

Official native desktop client for [Cabinet](https://github.com/soltros/Cabinet), written entirely in Rust.

Cabinet Desktop connects to Cabinet exclusively through its documented HTTP API. It does not embed the Cabinet web application, scrape HTML, or assume any particular hosted instance. Every user supplies the URL of their own Cabinet server.

## Architecture

Cabinet Desktop is a pure Rust application:

- **egui / eframe** — cross-platform desktop UI
- **reqwest** — Cabinet API client
- **keyring** — secure operating-system credential storage
- **tray-icon** — persistent system tray support
- **rfd** — native file picker/save dialogs
- **serde / serde_json** — API and local configuration serialization

There is no React, JavaScript, TypeScript, Electron, Tauri webview, or browser runtime in the application.

## Supported platforms

The client is being built for:

- Linux
- Windows
- macOS

Cross-platform CI compiles and checks the application on all three operating systems.

## Current API coverage

The desktop client currently implements the Cabinet API for:

### Authentication

- login
- session restoration
- logout
- current-user refresh

The server URL and username are stored in the normal application config directory.

The Cabinet API token is stored separately in the operating system credential store:

- Linux — Secret Service
- macOS — Keychain
- Windows — Credential Manager

### Files and folders

- file listing
- folder listing
- grid view
- list view
- search
- multi-file upload
- streamed file download
- create folder
- rename file
- delete file
- internal sharing with another Cabinet user
- public share-link creation

Uploads and downloads are streamed through Rust rather than loading an entire file into memory.

### Administration

When the authenticated Cabinet account has the `admin` role, Cabinet Desktop exposes:

- global server statistics
- user listing
- public-share listing
- server logs

Additional admin actions will be added as the interface reaches full parity with the Cabinet web application.

## Connecting to Cabinet

On first launch, enter the base URL of your Cabinet instance.

Example:

```text
https://cabinet.example.com
```

Do **not** append `/api`. Cabinet Desktop adds the documented API paths itself.

No Cabinet instance is hardcoded into the application.

## System tray

Cabinet Desktop is designed to remain available in the background.

Closing the main window hides it to the system tray instead of terminating the process.

The tray menu provides:

- **Show Cabinet** — restores and focuses the main window
- **Quit Cabinet** — exits the application completely

## Building from source

Install a current stable Rust toolchain.

Then:

```bash
cargo build
cargo run
```

For an optimized build:

```bash
cargo build --release
```

## Testing on NixOS

A development package is maintained in [soltros_nixpkgs](https://github.com/soltros/soltros_nixpkgs).

Until that package branch is merged, you can build or run the current development package directly from its branch:

```bash
nix build "github:soltros/soltros_nixpkgs/feat/cabinet-desktop#cabinet-desktop"
```

or:

```bash
nix run "github:soltros/soltros_nixpkgs/feat/cabinet-desktop#cabinet-desktop"
```

After the package lands on `main`, the normal commands become:

```bash
nix build github:soltros/soltros_nixpkgs#cabinet-desktop
nix run github:soltros/soltros_nixpkgs#cabinet-desktop
```

The Nix package is pinned to a specific Cabinet Desktop commit for reproducibility.

The `soltros_nixpkgs` update scanner compares that pin with the latest commit on this repository's `main` branch and opens or updates the normal `package-updates` issue when the package falls behind.

## Distribution targets

Planned release packaging includes:

### Linux

- DEB
- RPM
- AppImage
- AUR
- Nix / NixOS through `soltros_nixpkgs`

### Windows

- MSI
- NSIS installer

### macOS

- application bundle
- DMG

## Development branch

Initial implementation work is currently taking place on:

```text
feat/initial-desktop-client
```

This branch should be considered development software until the cross-platform CI and packaging checks are green.

## API contract

Cabinet Desktop deliberately keeps all server-side behavior in Cabinet itself.

The server remains authoritative for:

- authentication
- authorization
- quotas
- encryption
- folder ownership
- sharing rules
- administrator permissions

The desktop application is a client of the Cabinet API, not a second implementation of Cabinet's backend logic.

## License

GPL-3.0-only.
