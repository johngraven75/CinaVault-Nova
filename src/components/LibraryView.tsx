import { useMemo, useState } from "react";
import { DEMO_ITEMS, type MediaItem } from "../services/api";
import { VirtualList } from "./VirtualList";

function MediaCard({ item }: { item: MediaItem }) {
  const [menuOpen, setMenuOpen] = useState(false);
  return (
    <article className="glass card">
      {item.posterUrl ? (
        <img src={item.posterUrl} alt="" loading="lazy" />
      ) : (
        <div style={{ aspectRatio: "2 / 3", background: "#222", display: "grid", placeItems: "center" }}>
          <span className="muted">No poster</span>
        </div>
      )}
      <div className="card-body">
        <div className="row" style={{ justifyContent: "space-between" }}>
          <strong>{item.title}</strong>
          <div className="menu">
            <button aria-label="Media actions" onClick={() => setMenuOpen((v) => !v)}>
              ⋮
            </button>
            {menuOpen && (
              <div className="glass menu-list">
                <button onClick={() => setMenuOpen(false)}>Unmatch metadata</button>
                <button onClick={() => setMenuOpen(false)}>Match metadata</button>
                <button onClick={() => setMenuOpen(false)}>Lookup metadata</button>
              </div>
            )}
          </div>
        </div>
        <p className="muted">{item.year ?? "Unknown year"}</p>
        <p className="muted">{item.overview ?? ""}</p>
      </div>
    </article>
  );
}

export function LibraryView({ onLock }: { onLock: () => void }) {
  const [query, setQuery] = useState("");
  const items = useMemo(
    () =>
      DEMO_ITEMS.filter((i) =>
        i.title.toLowerCase().includes(query.toLowerCase()),
      ),
    [query],
  );

  return (
    <section>
      <div className="row" style={{ marginBottom: 16 }}>
        <input
          aria-label="Search library"
          placeholder="Search your library…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button onClick={onLock}>Lock vault</button>
      </div>
      <h2>Continue Watching</h2>
      <p className="muted">
        Shelf built from library items only. Items with no usable file path
        never appear here. A real scan replaces these demo items.
      </p>
      <VirtualList
        items={items}
        rowHeight={320}
        renderItem={(item) => <MediaCard key={item.id} item={item} />}
      />
      <div className="grid" style={{ marginTop: 16 }}>
        {items.map((item) => (
          <MediaCard key={item.id} item={item} />
        ))}
      </div>
    </section>
  );
}
