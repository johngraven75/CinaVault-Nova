import { useEffect, useState } from "react";
import { LibraryView } from "./components/LibraryView";
import { SettingsView } from "./components/SettingsView";
import { VaultGate } from "./components/VaultGate";
import { vaultLock, vaultStatus, type VaultStatus } from "./services/api";

type Tab = "library" | "settings";
type Mode = "phone" | "desktop" | "tv";

export default function App() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [tab, setTab] = useState<Tab>("library");
  const [mode, setMode] = useState<Mode>("desktop");
  const [highContrast, setHighContrast] = useState(false);

  useEffect(() => {
    vaultStatus().then(setStatus).catch(() => setStatus({ configured: false, unlocked: false }));
  }, []);

  if (status === null) {
    return (
      <div className="app">
        <p className="muted">Loading CinaVault-Nova…</p>
      </div>
    );
  }

  return (
    <div className={`app mode-${mode} ${highContrast ? "high-contrast" : ""}`}>
      <header>
        <h1>CinaVault-Nova</h1>
        <nav aria-label="Layout and theme">
          <label>
            Layout
            <select aria-label="Layout mode" value={mode} onChange={(e) => setMode(e.target.value as Mode)}>
              <option value="phone">Phone</option>
              <option value="desktop">Desktop</option>
              <option value="tv">TV (10-foot)</option>
            </select>
          </label>
          <button aria-pressed={highContrast} onClick={() => setHighContrast((v) => !v)}>
            {highContrast ? "High contrast: on" : "High contrast: off"}
          </button>
        </nav>
      </header>
      <p className="muted">v0.1.0 · independent rewrite · local-first</p>
      {!status.unlocked ? (
        <VaultGate
          configured={status.configured}
          onUnlocked={() => setStatus({ configured: true, unlocked: true })}
        />
      ) : (
        <>
          <nav aria-label="Main">
            <button onClick={() => setTab("library")}>Library</button>
            <button onClick={() => setTab("settings")}>Settings</button>
            <button
              onClick={async () => {
                const next = await vaultLock().catch(() => ({ configured: true, unlocked: false }));
                setStatus(next);
              }}
            >
              Lock vault
            </button>
          </nav>
          <main style={{ marginTop: 20 }}>
            {tab === "library" ? (
              <LibraryView
                onLock={async () => {
                  const next = await vaultLock().catch(() => ({ configured: true, unlocked: false }));
                  setStatus(next);
                }}
              />
            ) : (
              <SettingsView />
            )}
          </main>
        </>
      )}
    </div>
  );
}
