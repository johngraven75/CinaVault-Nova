# CinaVault Premium — Feature Parity Checklist

This checklist maps every CinaVault Premium feature to its status in CinaVault-Nova.

Status key:

- **Implemented** — working code path, exercised locally (test evidence below).
- **Scaffolded** — types, commands, UI screens, and/or stubs exist, but the full end-to-end behaviour is not yet complete.
- **Planned** — design documented, no code path yet.

Be honest about this list when reporting status. A scaffolded item is not shipped.

## Test evidence (this completion pass)

- Rust core: `cargo test -p cinavault-core --lib` — **26 passed, 0 failed**
  (vault seal/unseal + tamper, DB upsert/search/purge/attempts, provider
  merge, Lookup apply/unmatch, adult store-safe gating, VPN profile
  parsing, Drive PKCE/no-client-ID/media filter, playback sessions,
  DLNA discovery returning only real devices, jobs busy-guard, vault
  file seal/lock round-trip, updater version compare).
- Frontend: `npm run build` — **passed** (tsc --noEmit + Vite, 34 modules).
- Tauri shell: `cargo check -p cinavault-nova` — see report; on Linux
  it requires the GTK/WebKit sysroot (see below). Full Tauri binary
  build and live Windows run are still unverified.
- Live network tests (real provider APIs, Drive API, WireGuard
  installer download, casting to a real device) were NOT run; provider
  network code is implemented with generic JSON parsing and is
  exercised only via mock providers in unit tests.

## Library

| Feature | Status | Notes |
|---|---|---|
| Local folder scanning into a catalog | Implemented | `catalog::scan_folder` + DB upsert via `scan_library` command |
| SQLite-backed library database | Implemented | `db::LibraryDb` (rusqlite, bundled): items, watch state, adult attempts |
| Home shelves (Continue Watching, Trending, etc.) | Implemented | `continue_watching` + library-only shelves in LibraryView; Trending is library items only |
| Remove suggested movies not in the library | Implemented | Shelf construction filters to library items only; `purge_missing_files` purges only no-user-data rows, keeps user-data rows as missing |
| Search, filters, sorting | Implemented | DB `search` + frontend search/filter; indexed FTS is a future optimisation, not a missing feature |
| Watch state, favourites, progress, watchlist | Implemented | DB fields + `set_watch`; watchlist column present, UI toggle planned |

## Metadata

| Feature | Status | Notes |
|---|---|---|
| AI metadata query that never blocks library access | Implemented | Enrichment is a background job (`jobs::JobRegistry`, busy-guard, job id/status); no inline whole-library await |
| Automated poster + metadata gathering and saving | Implemented | Provider merge-until-title-and-poster, local poster saving (sidecar/cache), failed download never leaves a remote URL |
| Adult poster/metadata providers | Implemented | `adult.rs`: StashDB/TPDB/IAFD manifests, merge, durable attempts, chapter-frame (ffmpeg if present) + sidecar fallback; store-safe gating tested |
| Media-card 3-dot menu: Unmatch / Match / Lookup | Implemented | UI menu + `item_unmatch`, `item_lookup` (ranked candidates), `item_match` (apply chosen) commands end-to-end |
| Provider plugin manifests (versioned) | Implemented | `ProviderManifest` with id, version, kinds, store-safe flag; HTTP provider with generic JSON parsing |

## Vault, security, network

| Feature | Status | Notes |
|---|---|---|
| App-vault encryption, user-selected passphrase | Implemented | Argon2id + XChaCha20-Poly1305, salt + verification tag only, in-memory Zeroizing key |
| Vault setup at first run, unlock gate, lock | Implemented | Tauri commands + VaultGate; header persisted as `vault.json` |
| Full DB/artwork sealing on lock | Implemented | `vault_store::lock_vault_folder` seals DB + artwork, zero-overwrites + deletes plaintexts and session copy; session DB unsealed on unlock |
| Vault passphrase change + backup export/import | Implemented | `vault_change_passphrase` (re-seals sealed DB), backup = header + sealed DB only (no plaintext/artwork), import verifies passphrase before overwriting |
| Lost passphrase = unrecoverable, no backdoor | Implemented | Stated in code, UI, and README; no recovery key held |
| WireGuard VPN install + profile | Implemented (Windows install) / Scaffolded (live test) | Pinned 1.1.1 MSI, SHA-256 audit (not pinned — WireGuard publishes none), Authenticode check on Windows, elevated msiexec, distinct errors 20/21/22/23/1223, .conf parsing; live Windows install test NOT run, non-Windows returns distinct unsupported error, never fake success |
| Google Drive reconnect (OAuth) | Implemented | PKCE + loopback 127.0.0.1:19284, refresh-on-401 with one retry then disconnected, revoke on disconnect; client ID never invented — empty/placeholder is a plain error |
| Drive media listing / ingestion | Implemented | `drive_list` (media-filtered) + `drive_ingest` (download to local folder, scan into library with `drive:` id); live Drive API test NOT run |

## Playback and casting

| Feature | Status | Notes |
|---|---|---|
| Hardware-accelerated playback | Scaffolded | Decode preference reported to the platform player; the core does not decode video itself and does not claim to |
| HLS / DASH playback | Scaffolded | Detection by extension (.m3u8/.mpd) + session kinds; handed to a supporting player, no in-core MSE implementation |
| Chromecast / AirPlay / DLNA casting | Scaffolded | DLNA discovery is real (SSDP M-SEARCH, no fabricated devices); Chromecast/AirPlay sessions require a platform-supplied device location, never invented |
| Offline download and resume across devices | Implemented (download) / Scaffolded (cross-device) | HTTP download with Range resume + progress; resume position via watch state; cross-device sync of that state is design-only (sync payload types exist) |

## Platform and distribution

| Feature | Status | Notes |
|---|---|---|
| Windows installers (NSIS + MSI) | Scaffolded | Release workflow builds them on a `v*` tag; parent handles the release for this pass — no tag/release created here |
| Signed installers | Planned | Blocked on signing secrets; `updater::SIGNING_SECRETS_NEEDED` lists names only, no values |
| Delta auto-updates, store-ready builds | Scaffolded | Update *check* implemented (GitHub releases compare); delta apply requires signed updater manifest, so `delta_update_available` is honestly false until signing exists |
| iOS, Android, Debian Linux/Zorin Premium versions | Planned | Deferred; same Rust core, thin platform shells |
| Accessibility: keyboard, screen reader, high-contrast | Implemented (UI) / Scaffolded (audit) | Semantic labels, aria menus/dialogs, keyboard focus outlines, phone/desktop/TV layout modes, high-contrast theme; a full accessibility audit is still planned |
| Sync (watch state / collections, encrypted) | Scaffolded | Payload types + sealing design; no sync server/transport wired |
