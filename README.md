# Cabinet Desktop

A native desktop client for [Cabinet](https://github.com/soltros/Cabinet), written entirely in Rust.

Cabinet Desktop connects to the documented Cabinet HTTP API. It does not embed the web application, scrape pages, or assume a specific hosted Cabinet instance. On first launch, each user supplies the URL of their own Cabinet server.

## Technology

- **Rust** for the complete application
- **egui/eframe** for the cross-platform desktop interface
- **reqwest** for the Cabinet API
- **keyring** for secure OS credential storage
- **tray-icon** for persistent system-tray integration
- **rfd** for native file dialogs

There is no React, JavaScript, TypeScript, Electron, or browser shell in the application.

## Current functionality

The initial client implements the Cabinet API for:

- login and session restoration
- files and folders
- grid and list views
- search
- multi-file upload
- streamed file downloads
- creating folders
- renaming and deleting files
- internal user sharing
- creating public share links
- administrator statistics, users, shares and server logs
- persistent tray behavior

Closing the main window hides Cabinet to the system tray. Use **Show Cabinet** to restore it or **Quit Cabinet** to exit completely.

## Server configuration

The server is never hardcoded. Enter a Cabinet base URL such as:

```text
https://cabinet.example.com
```

Do not append `/api`; Cabinet Desktop adds API paths itself.

The server URL and username are stored in the app configuration directory. The session token is stored using the operating system credential store:

- Linux: Secret Service
- macOS: Keychain
- Windows: Credential Manager

## Building

Install a stable Rust toolchain and the platform dependencies required by eframe/tray-icon.

```bash
cargo build
cargo run
```

For an optimized build:

```bash
cargo build --release
```

## Packaging roadmap

Release automation will produce:

- Windows: MSI/NSIS
- macOS: app bundle/DMG
- Linux: DEB, RPM, AppImage
- Arch Linux: AUR PKGBUILD
- NixOS/Nix: package through `soltros/soltros_nixpkgs`

## API contract

Cabinet Desktop is intentionally API-driven. The server remains authoritative for authentication, authorization, quotas, encryption, shares and administration.

## License

GPL-3.0-only.
