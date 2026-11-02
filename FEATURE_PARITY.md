# CinaVault Premium — Feature Parity Checklist

This checklist maps every CinaVault Premium feature to its status in CinaVault-Nova v0.1.0.

Status key:

- **Implemented** — working code path, exercised locally (or stated as verified in the release notes).
- **Scaffolded** — types, commands, UI screens, and/or stubs exist, but the full end-to-end behaviour is not yet complete.
- **Planned** — design documented, no code path yet.

Be honest about this list when reporting status. A scaffolded item is not shipped.

## Library

| Feature | Status | Notes |
|---|---|---|
| Local folder scanning into a catalog | Scaffolded | `cinavault-core::catalog` scans folders, builds `MediaItem` records, skips files with no usable path |
| SQLite-backed library database | Scaffolded | Catalog types and scan command exist; persistent DB wiring is planned |
| Home shelves (Continue Watching, Trending, etc.) | Scaffolded | Frontend library view with shelves; Trending uses library items only (no non-library suggestions) |
| Remove suggested movies not in the library | Scaffolded | Shelf construction filters to library items only; a full missing-file reconciliation pass is planned |
| Search, filters, sorting | Scaffolded | Frontend search/filter UI; indexed search is planned |
| Watch state, favourites, progress, watchlist | Scaffolded | Types and sync design exist; persistence is planned |

## Metadata

| Feature | Status | Notes |
|---|---|---|
| AI metadata query that never blocks library access | Scaffolded | Enrichment is designed as a background job with a job id and status; the blocking inline pattern from Premium is deliberately not copied |
| Automated poster + metadata gathering and saving | Scaffolded | Provider plugin manifest + merge-until-title-and-poster design; provider network calls are planned |
| Adult poster/metadata providers | Scaffolded | Provider manifest supports adult providers behind a store-safe gate; no provider network code yet |
| Media-card 3-dot menu: Unmatch / Match / Lookup | Scaffolded | UI menu and Tauri commands exist; Lookup picker wiring to providers is planned |
| Provider plugin manifests (versioned) | Implemented (design) | `cinavault-core::providers::ProviderManifest` with id, version, kinds, and store-safe flag |

## Vault, security, network

| Feature | Status | Notes |
|---|---|---|
| App-vault encryption, user-selected passphrase | Scaffolded | `cinavault-core::vault` implements Argon2id + XChaCha20-Poly1305 seal/unseal with salt + verification tag only; startup gate UI exists; full DB/artwork sealing on lock is planned |
| Vault setup at first run, unlock gate, lock | Scaffolded | Tauri commands `vault_setup`, `vault_unlock`, `vault_lock`, `vault_status` + VaultGate screen |
| Lost passphrase = unrecoverable, no backdoor | Implemented (design) | Stated in code, UI, and README; no recovery key is held by the app |
| WireGuard VPN install + profile | Planned | Design only: pinned official installer, signature verification, elevated install; no VPN code in v0.1.0 |
| Google Drive reconnect (OAuth) | Planned | Design only: PKCE + loopback OAuth, refresh-on-401, true disconnected state; no OAuth code in v0.1.0 |
| Drive media listing / ingestion | Planned | Follows OAuth; not in v0.1.0 |

## Playback and casting

| Feature | Status | Notes |
|---|---|---|
| Hardware-accelerated playback | Planned | Playback pipeline stub only |
| HLS / DASH playback | Planned | Pipeline stub only |
| Chromecast / AirPlay / DLNA casting | Planned | Design only; no casting code in v0.1.0 |
| Offline download and resume across devices | Planned | Resume types exist in sync design; download/resume implementation is planned |

## Platform and distribution

| Feature | Status | Notes |
|---|---|---|
| Windows installers (NSIS + MSI) | Scaffolded | Release workflow builds them on a `v*` tag; see the v0.1.0 release for actual assets |
| Signed installers | Planned | Blocked on signing secrets; v0.1.0 installers are unsigned |
| Delta auto-updates, store-ready builds | Planned | Tauri updater design; not wired in v0.1.0 |
| iOS, Android, Debian Linux/Zorin Premium versions | Planned | Deferred; same Rust core, thin platform shells |
| Accessibility: keyboard, screen reader, high-contrast | Scaffolded | Semantic UI structure and high-contrast glassmorphism theme; full audit is planned |
