# CinaVault-Nova

An independent rewrite of CinaVault — a local-first media library for Windows (with iOS, Android, and Debian Linux/Zorin Premium versions planned later).

**This is a new, independent project.** It is not CinaVault-Premium, not a fork of it, and it shares no code history with it. It is built clean, from scratch, around a shared Rust core.

## Architecture

- **Rust core (`cinavault-core`)** — media scanning and catalog, metadata provider plugins, playback pipeline, app-vault encryption, and sync. One core, thin platform shells.
- **Desktop shell (`src-tauri`)** — Tauri 2. Small, secure, native installers.
- **Frontend (`src/`)** — React + TypeScript with a glassmorphism library UI: virtualised lists, lazy poster loading, a vault gate screen, and settings.
- **Mobile/TV path (planned)** — Tauri Mobile with phone, desktop, and TV 10-foot modes, on the same Rust core.

## Principles

- **Local-first, private by default.** Your library lives on your machine. Cloud sync, if enabled, covers watch state and collections only — not raw media — and is end-to-end encrypted.
- **App-vault encryption, passphrase-first.** At first run you choose your own passphrase. The vault is encrypted with Argon2id + XChaCha20-Poly1305. Only a salt and a verification tag are stored — never the passphrase or the derived key. A wrong passphrase means locked, with no partial access. **Losing your passphrase means the vault is unrecoverable by design. There is no backdoor.**
- **Providers as plugins.** Metadata, poster, and adult providers are versioned plugin manifests. A dead provider must not break the app, and a new provider must not require a full release.
- **On-device AI first.** Enrichment and recommendations run locally where possible. Cloud AI is opt-in only. Background jobs never block the UI.
- **Playback and casting.** Hardware-accelerated playback, HLS/DASH, Chromecast, AirPlay, and DLNA casting, offline download, and resume across devices.

## Status — read this before judging completeness

This is v0.1.0 with the core completed as a working library app: SQLite
library DB, provider/adult enrichment with store-safe gating, vault
sealing + passphrase change/backup, WireGuard install/profile, Drive
OAuth + listing/ingestion, and offline download — all exercised by
Rust core tests (26/26) and a passing frontend build. See
[FEATURE_PARITY.md](FEATURE_PARITY.md) for an honest, item-by-item
status: Implemented / Scaffolded / Planned. Do not treat a scaffolded
item as shipped. Still unverified: full Tauri binary build, live
Windows run, live provider/Drive/WireGuard network tests, signing,
and hardware playback/casting against real devices.

## Build

Prerequisites: Node.js 20+, Rust (stable), and the Tauri prerequisites for your platform.

```bash
npm install
npm run build        # frontend only
npm run tauri:build  # full Tauri build (requires Rust + Tauri CLI)
```

On Windows, the release workflow (`.github/workflows/release.yml`) builds NSIS and MSI installers on a `v*` tag and attaches them to the GitHub release.

## Security note

Installers in the v0.1.0 release are **unsigned** until code-signing secrets exist. Verify downloads against the release's published checksums. Never commit signing credentials, API keys, or passphrases to this repo.

## App identity

- App identifier: `com.cinavault.nova`
- Version: `0.1.0`
