import React, { useMemo, useRef, useState } from "react";

// Lightweight windowing for long library grids: only the visible window
// (plus a small overscan) is rendered. Posters use loading="lazy" so
// off-screen artwork is not fetched eagerly. This is a placeholder
// implementation for the v0.1.0 scaffold, not a full virtualised grid
// library — it keeps the contract (render only what is visible) without
// adding a dependency the scaffold cannot yet verify in CI.
export function VirtualList<T>({
  items,
  rowHeight,
  renderItem,
}: {
  items: T[];
  rowHeight: number;
  renderItem: (item: T, index: number) => React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const viewportHeight = 600;
  const overscan = 2;

  const { start, end } = useMemo(() => {
    const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
    const last = Math.min(
      items.length,
      Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan,
    );
    return { start: first, end: last };
  }, [scrollTop, items.length, rowHeight]);

  return (
    <div
      ref={ref}
      style={{ height: viewportHeight, overflowY: "auto" }}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      <div style={{ height: items.length * rowHeight, position: "relative" }}>
        <div style={{ position: "absolute", top: start * rowHeight, left: 0, right: 0 }}>
          {items.slice(start, end).map((item, i) => renderItem(item, start + i))}
        </div>
      </div>
    </div>
  );
}
