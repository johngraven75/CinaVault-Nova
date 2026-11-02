import { useState } from "react";
import { vaultSetup, vaultUnlock } from "../services/api";

export function VaultGate({
  configured,
  onUnlocked,
}: {
  configured: boolean;
  onUnlocked: () => void;
}) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");

  async function submit() {
    setError("");
    if (passphrase.length < 8) {
      setError("Choose a passphrase of at least 8 characters — longer is strongly recommended.");
      return;
    }
    if (!configured && passphrase !== confirm) {
      setError("Passphrases do not match.");
      return;
    }
    try {
      const status = configured
        ? await vaultUnlock(passphrase)
        : await vaultSetup(passphrase);
      if (status.unlocked) onUnlocked();
      else setError("Wrong passphrase. The vault stays locked.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Wrong passphrase. The vault stays locked.");
    }
  }

  return (
    <section className="glass">
      <h2>{configured ? "Unlock your vault" : "Set up your vault"}</h2>
      <p className="muted">
        Your passphrase is the only key. There is no backdoor and no recovery
        key held by this app.
      </p>
      <div className="glass warning">
        <strong>Warning:</strong> if you lose your passphrase, your encrypted
        library is unrecoverable by design. Store it somewhere safe, separate
        from this device, before you continue.
      </div>
      <div style={{ display: "grid", gap: 12, marginTop: 16, maxWidth: 480 }}>
        <input
          type="password"
          placeholder={configured ? "Your passphrase" : "Choose a passphrase"}
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          autoComplete="new-password"
        />
        {!configured && (
          <input
            type="password"
            placeholder="Confirm passphrase"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="new-password"
          />
        )}
        {error && <p role="alert">{error}</p>}
        <button className="primary" onClick={submit}>
          {configured ? "Unlock" : "Create vault"}
        </button>
      </div>
    </section>
  );
}
