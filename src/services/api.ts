// Tauri command bridge. In a plain web build (no Tauri runtime) these
// resolve to the scaffold fallbacks below, so `npm run build` and a web
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
}

type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function getInvoke(): TauriInvoke | null {
  const w = window as unknown as {
    __TAURI__?: { core: { invoke: TauriInvoke } };
  };
  return w.__TAURI__?.core.invoke ?? null;
}

export async function vaultStatus(): Promise<VaultStatus> {
  const invoke = getInvoke();
  if (!invoke) return { configured: false, unlocked: false };
  return (await invoke("vault_status")) as VaultStatus;
}

export async function vaultSetup(passphrase: string): Promise<VaultStatus> {
  const invoke = getInvoke();
  if (!invoke) return { configured: true, unlocked: true };
  return (await invoke("vault_setup", { passphrase })) as VaultStatus;
}

export async function vaultUnlock(passphrase: string): Promise<VaultStatus> {
  const invoke = getInvoke();
  if (!invoke) return { configured: true, unlocked: true };
  return (await invoke("vault_unlock", { passphrase })) as VaultStatus;
}

export async function vaultLock(): Promise<VaultStatus> {
  const invoke = getInvoke();
  if (!invoke) return { configured: true, unlocked: false };
  return (await invoke("vault_lock")) as VaultStatus;
}

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
