import { useState } from "react";
import {
  castDiscoverDlna,
  driveAuthUrl,
  driveDisconnect,
  driveList,
  signingRequirements,
  updateCheck,
  vaultChangePassphrase,
  vaultExportBackup,
  vaultImportBackup,
  vpnInstall,
  vpnProfile,
  type DriveFile,
} from "../services/api";

export function SettingsView() {
  const [notice, setNotice] = useState("");
  const [oldPass, setOldPass] = useState("");
  const [newPass, setNewPass] = useState("");
  const [backupFolder, setBackupFolder] = useState("");
  const [confPath, setConfPath] = useState("");
  const [driveFiles, setDriveFiles] = useState<DriveFile[]>([]);
  const [dlna, setDlna] = useState<unknown[]>([]);
  const [signing, setSigning] = useState<string[]>([]);

  return (
    <section className="glass" aria-label="Settings">
      <h2>Settings</h2>
      {notice && <p className="muted">{notice}</p>}

      <h3>Vault</h3>
      <p className="muted">
        Losing your passphrase means the vault is unrecoverable by design.
        There is no backdoor. A backup still requires your passphrase and
        contains no plaintext or artwork.
      </p>
      <div className="row">
        <input aria-label="Current passphrase" type="password" placeholder="Current passphrase" value={oldPass} onChange={(e) => setOldPass(e.target.value)} />
        <input aria-label="New passphrase" type="password" placeholder="New passphrase (8+ characters)" value={newPass} onChange={(e) => setNewPass(e.target.value)} />
        <button onClick={async () => { await vaultChangePassphrase(oldPass, newPass); setNotice("Passphrase changed. The vault is now locked; unlock with the new passphrase."); }}>
          Change passphrase
        </button>
      </div>
      <div className="row" style={{ marginTop: 12 }}>
        <input aria-label="Backup folder" placeholder="Backup folder (outside the vault)" value={backupFolder} onChange={(e) => setBackupFolder(e.target.value)} />
        <button onClick={async () => setNotice(await vaultExportBackup(backupFolder))}>Export backup</button>
        <button onClick={async () => { await vaultImportBackup(backupFolder, oldPass); setNotice("Backup imported. Unlock with its passphrase."); }}>
          Import backup
        </button>
      </div>

      <h3>Providers</h3>
      <p className="muted">
        Provider plugins are versioned manifests. A dead provider degrades
        gracefully and never blocks the library. Store-safe builds never use
        adult-only providers, and the adult pass is skipped entirely there.
      </p>

      <h3>WireGuard VPN</h3>
      <p className="muted">
        Install downloads the pinned official WireGuard 1.1.1 MSI, verifies
        Authenticode on Windows, and installs elevated. A tunnel still
        requires your own .conf file. No server-peer issuance is done here.
      </p>
      <div className="row">
        <button onClick={async () => setNotice(JSON.stringify(await vpnInstall()))}>Install WireGuard</button>
        <input aria-label="WireGuard profile path" placeholder="Path to .conf profile" value={confPath} onChange={(e) => setConfPath(e.target.value)} />
        <button onClick={async () => setNotice(JSON.stringify(await vpnProfile(confPath)))}>Load profile</button>
      </div>

      <h3>Google Drive</h3>
      <p className="muted">
        Drive needs a Desktop-app OAuth client ID (Drive API enabled,
        redirect http://127.0.0.1:19284/auth/callback). Until one is
        configured, Drive is disconnected — no client ID is invented, and
        the local-folder flow is separate and labelled local only.
      </p>
      <div className="row">
        <button onClick={async () => { const url = await driveAuthUrl(); setNotice(url || "No client ID configured — Drive is disconnected."); }}>
          Connect Drive (OAuth)
        </button>
        <button onClick={async () => setDriveFiles(await driveList())}>List Drive media</button>
        <button onClick={async () => { await driveDisconnect(); setDriveFiles([]); setNotice("Drive disconnected and token revoked."); }}>
          Disconnect Drive
        </button>
      </div>
      {driveFiles.length > 0 && (
        <ul>
          {driveFiles.map((f) => (
            <li key={f.id}>{f.name} ({f.mimeType})</li>
          ))}
        </ul>
      )}

      <h3>Playback &amp; casting</h3>
      <p className="muted">
        Hardware-accelerated decoding is requested from the platform player;
        HLS (.m3u8) and DASH (.mpd) are detected by extension. DLNA
        discovery below is a real SSDP search — on a network with no
        renderers it honestly returns nothing.
      </p>
      <button onClick={async () => setDlna(await castDiscoverDlna())}>Discover DLNA devices</button>
      {dlna.length > 0 && <pre>{JSON.stringify(dlna, null, 2)}</pre>}

      <h3>Updates &amp; signing</h3>
      <p className="muted">
        Delta auto-updates require a signed updater manifest. Until signing
        secrets exist, updates are manual (download the new installer).
      </p>
      <div className="row">
        <button onClick={async () => setNotice(JSON.stringify(await updateCheck()))}>Check for updates</button>
        <button onClick={async () => setSigning(await signingRequirements())}>Signing secrets needed</button>
      </div>
      {signing.length > 0 && <p className="muted">{signing.join(", ")}</p>}

      <h3>Appearance &amp; accessibility</h3>
      <p className="muted">
        Use the mode and theme controls in the header: phone, desktop, and
        TV layouts, plus a high-contrast theme. Every screen uses semantic
        labels and keyboard-reachable controls.
      </p>
    </section>
  );
}
