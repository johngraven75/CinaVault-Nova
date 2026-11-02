export function SettingsView() {
  return (
    <section className="glass">
      <h2>Settings</h2>
      <h3>Vault</h3>
      <p className="muted">
        Vault status, lock, passphrase change, and backup export/import land
        here in the full implementation. v0.1.0 ships the vault gate and
        lock command; change-passphrase and backup are scaffolded in the
        core design and marked as such in FEATURE_PARITY.md.
      </p>
      <h3>Providers</h3>
      <p className="muted">
        Provider plugins are versioned manifests. A dead provider degrades
        gracefully and never blocks the library. Store-safe builds never use
        adult-only providers.
      </p>
      <h3>Playback &amp; casting</h3>
      <p className="muted">
        Hardware playback, HLS/DASH, Chromecast, AirPlay, and DLNA are
        planned. This scaffold defines the playback pipeline contract only.
      </p>
      <h3>Appearance &amp; accessibility</h3>
      <p className="muted">
        High-contrast glassmorphism theme, keyboard navigation, and
        screen-reader labels are design requirements for every screen.
      </p>
    </section>
  );
}
