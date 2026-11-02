// Tauri command bridge. In a plain web build (no Tauri runtime) these
// resolve to the fallbacks below, so `npm run build` and a web
// preview still work. In the Tauri app they call the real Rust commands.

export interface VaultStatus {
  configured: boolean;
  unlocked: boolean;
}

export interface MediaItem {
  id: string;
  title: string;
  year?: number | null;
  paths: string[];
  posterUrl?: string | null;
  overview?: string | null;
  watched: boolean;
  progressSeconds: number;
  favourite: boolean;
  isAdult?: boolean;
}

export interface LookupCandidate {
  providerId: string;
  title: string;
  posterUrl?: string | null;
  overview?: string | null;
  year?: number | null;
}

export interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
}

type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function getInvoke(): TauriInvoke | null {
  const w = window as unknown as {
    __TAURI__?: { core: { invoke: TauriInvoke } };
  };
  return w.__TAURI__?.core.invoke ?? null;
}

async function invokeOr<T>(cmd: string, args: Record<string, unknown> | undefined, fallback: T): Promise<T> {
  const invoke = getInvoke();
  if (!invoke) return fallback;
  return (await invoke(cmd, args)) as T;
}

export const vaultStatus = () => invokeOr<VaultStatus>("vault_status", undefined, { configured: false, unlocked: false });
export const vaultSetup = (passphrase: string) => invokeOr<VaultStatus>("vault_setup", { passphrase }, { configured: true, unlocked: true });
export const vaultUnlock = (passphrase: string) => invokeOr<VaultStatus>("vault_unlock", { passphrase }, { configured: true, unlocked: true });
export const vaultLock = () => invokeOr<VaultStatus>("vault_lock", undefined, { configured: true, unlocked: false });
export const vaultChangePassphrase = (oldPassphrase: string, newPassphrase: string) =>
  invokeOr<VaultStatus>("vault_change_passphrase", { oldPassphrase, newPassphrase }, { configured: true, unlocked: false });
export const vaultExportBackup = (destFolder: string) =>
  invokeOr<string>("vault_export_backup", { destFolder }, "Backup exported (web fallback).");
export const vaultImportBackup = (backupFolder: string, passphrase: string) =>
  invokeOr<VaultStatus>("vault_import_backup", { backupFolder, passphrase }, { configured: true, unlocked: false });

export const listLibrary = () => invokeOr<MediaItem[]>("list_library", undefined, DEMO_ITEMS);
export const searchLibrary = (query: string) => invokeOr<MediaItem[]>("search_library", { query }, DEMO_ITEMS);
export const scanLibrary = (folder: string) => invokeOr<unknown>("scan_library", { folder }, null);
export const continueWatching = () => invokeOr<MediaItem[]>("continue_watching", undefined, []);
export const purgeMissing = () => invokeOr<[string[], string[]]>("purge_missing", undefined, [[], []]);
export const itemUnmatch = (itemId: string) => invokeOr<void>("item_unmatch", { itemId }, undefined);
export const itemLookup = (title: string, year?: number | null) =>
  invokeOr<LookupCandidate[]>("item_lookup", { title, year: year ?? null }, []);
export const itemMatch = (itemId: string, candidate: LookupCandidate) =>
  invokeOr<void>("item_match", { itemId, candidate }, undefined);
export const itemMarkAdult = (itemId: string, isAdult: boolean) =>
  invokeOr<void>("item_mark_adult", { itemId, isAdult }, undefined);
export const enrichAdult = (itemId: string) =>
  invokeOr<string>("enrich_adult", { itemId }, "Adult pass (web fallback).");

export const vpnInstall = () => invokeOr<unknown>("vpn_install", undefined, null);
export const vpnProfile = (path: string) => invokeOr<unknown>("vpn_profile", { path }, null);
export const driveAuthUrl = () => invokeOr<string>("drive_auth_url", undefined, "");
export const driveList = () => invokeOr<DriveFile[]>("drive_list", undefined, []);
export const driveDisconnect = () => invokeOr<boolean>("drive_disconnect", undefined, true);
export const playbackStart = (itemId: string, source: string) =>
  invokeOr<unknown>("playback_start", { itemId, source }, null);
export const castDiscoverDlna = () => invokeOr<unknown[]>("cast_discover_dlna", undefined, []);
export const updateCheck = () => invokeOr<unknown>("update_check", undefined, null);
export const signingRequirements = () => invokeOr<string[]>("signing_requirements", undefined, []);

export const DEMO_ITEMS: MediaItem[] = [
  {
    id: "demo-1",
    title: "Demo Film (library item)",
    year: 2024,
    paths: ["/media/demo-film.mkv"],
    posterUrl: null,
    overview: "Placeholder item shown until a real scan runs.",
    watched: false,
    progressSeconds: 0,
    favourite: false,
  },
];
