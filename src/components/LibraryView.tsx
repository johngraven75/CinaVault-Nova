import { useEffect, useMemo, useState } from "react";
import {
  continueWatching,
  enrichAdult,
  itemLookup,
  itemMarkAdult,
  itemMatch,
  itemUnmatch,
  listLibrary,
  purgeMissing,
  scanLibrary,
  type LookupCandidate,
  type MediaItem,
} from "../services/api";
import { VirtualList } from "./VirtualList";

function MediaCard({ item, onRefresh }: { item: MediaItem; onRefresh: () => void }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [candidates, setCandidates] = useState<LookupCandidate[] | null>(null);
  const [status, setStatus] = useState("");
  return (
    <article className="glass card" aria-label={item.title}>
      {item.posterUrl ? (
        <img src={item.posterUrl} alt={`${item.title} poster`} loading="lazy" />
      ) : (
        <div style={{ aspectRatio: "2 / 3", background: "#222", display: "grid", placeItems: "center" }}>
          <span className="muted">No poster</span>
        </div>
      )}
      <div className="card-body">
        <div className="row" style={{ justifyContent: "space-between" }}>
          <strong>{item.title}</strong>
          <div className="menu">
            <button aria-label={`Media actions for ${item.title}`} aria-haspopup="menu" onClick={() => setMenuOpen((v) => !v)}>
              ⋮
            </button>
            {menuOpen && (
              <div className="glass menu-list" role="menu">
                <button onClick={async () => { await itemUnmatch(item.id); setMenuOpen(false); onRefresh(); }}>Unmatch metadata</button>
                <button
                  onClick={async () => {
                    const cands = await itemLookup(item.title, item.year ?? null);
                    setCandidates(cands);
                    setMenuOpen(false);
                  }}
                >
                  Lookup metadata
                </button>
                <button
                  onClick={async () => {
                    await itemMarkAdult(item.id, !item.isAdult);
                    setMenuOpen(false);
                    onRefresh();
                  }}
                >
                  {item.isAdult ? "Unmark adult" : "Mark adult"}
                </button>
                {item.isAdult && (
                  <button
                    onClick={async () => {
                      const r = await enrichAdult(item.id);
                      setStatus(String(r));
                      setMenuOpen(false);
                      onRefresh();
                    }}
                  >
                    Enrich adult metadata
                  </button>
                )}
              </div>
            )}
          </div>
        </div>
        <p className="muted">{item.year ?? "Unknown year"}</p>
        <p className="muted">{item.overview ?? ""}</p>
        {status && <p className="muted">{status}</p>}
        {candidates && (
          <div className="glass" role="dialog" aria-label="Lookup results">
            <h3>Lookup results</h3>
            {candidates.length === 0 && <p className="muted">No candidates found.</p>}
            {candidates.map((c, i) => (
              <div key={i} className="row" style={{ justifyContent: "space-between" }}>
                <span>
                  {c.title} {c.year ? `(${c.year})` : ""} — {c.providerId}
                </span>
                <button
                  className="primary"
                  onClick={async () => {
                    await itemMatch(item.id, c);
                    setCandidates(null);
                    onRefresh();
                  }}
                >
                  Match
                </button>
              </div>
            ))}
            <button onClick={() => setCandidates(null)}>Close</button>
          </div>
        )}
      </div>
    </article>
  );
}

export function LibraryView({ onLock }: { onLock: () => void }) {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<MediaItem[]>([]);
  const [watching, setWatching] = useState<MediaItem[]>([]);
  const [folder, setFolder] = useState("");
  const [notice, setNotice] = useState("");

  const refresh = async () => {
    const all = await listLibrary().catch(() => []);
    setItems(all);
    const cw = await continueWatching().catch(() => []);
    setWatching(cw);
  };

  useEffect(() => {
    refresh().catch(() => undefined);
  }, []);

  const filtered = useMemo(
    () => items.filter((i) => i.title.toLowerCase().includes(query.toLowerCase())),
    [items, query],
  );

  return (
    <section aria-label="Library">
      <div className="row" style={{ marginBottom: 16 }}>
        <input
          aria-label="Search library"
          placeholder="Search your library…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button onClick={onLock}>Lock vault</button>
      </div>
      <div className="row" style={{ marginBottom: 16 }}>
        <input
          aria-label="Library folder to scan"
          placeholder="Folder to scan, e.g. C:\Media"
          value={folder}
          onChange={(e) => setFolder(e.target.value)}
        />
        <button
          className="primary"
          onClick={async () => {
            await scanLibrary(folder);
            await refresh();
            setNotice("Scan complete.");
          }}
        >
          Scan folder
        </button>
        <button
          onClick={async () => {
            const [purged, kept] = await purgeMissing();
            setNotice(`Purged ${purged.length} missing items; kept ${kept.length} with user data.`);
            await refresh();
          }}
        >
          Reconcile missing files
        </button>
      </div>
      {notice && <p className="muted">{notice}</p>}
      <h2>Continue Watching</h2>
      {watching.length === 0 && <p className="muted">Nothing in progress.</p>}
      <div className="grid" style={{ marginTop: 8 }}>
        {watching.map((item) => (
          <MediaCard key={item.id} item={item} onRefresh={refresh} />
        ))}
      </div>
      <h2 style={{ marginTop: 24 }}>Library</h2>
      <p className="muted">
        Shelves are built from library items only. Items with no usable file
        path never appear here.
      </p>
      <VirtualList
        items={filtered}
        rowHeight={320}
        renderItem={(item) => <MediaCard key={item.id} item={item} onRefresh={refresh} />}
      />
    </section>
  );
}
