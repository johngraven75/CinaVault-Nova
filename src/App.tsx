import { useEffect, useState } from "react";
import { LibraryView } from "./components/LibraryView";
import { SettingsView } from "./components/SettingsView";
import { VaultGate } from "./components/VaultGate";
import { vaultLock, vaultStatus, type VaultStatus } from "./services/api";

type Tab = "library" | "settings";

export default function App() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [tab, setTab] = useState<Tab>("library");

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

  if (!status.unlocked) {
    return (
      <div className="app">
        <header>
          <h1>CinaVault-Nova</h1>
          <span className="muted">v0.1.0 · independent rewrite</span>
        </header>
        <VaultGate
          configured={status.configured}
          onUnlocked={() => setStatus({ configured: true, unlocked: true })}
        />
      </div>
    );
  }

  return (
    <div className="app">
      <header>
        <h1>CinaVault-Nova</h1>
        <nav>
          <button onClick={() => setTab("library")}>Library</button>
          <button onClick={() => setTab("settings")}>Settings</button>
        </nav>
      </header>
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
    </div>
  );
}
