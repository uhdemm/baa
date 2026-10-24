// Small controls from the design system: Toggle, Select, Combobox (searchable), Keycap, Row, Icon.
import { ReactNode, useEffect, useMemo, useRef, useState } from "react";

export function Toggle({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} className={`toggle ${on ? "on" : ""}`} onClick={() => onChange(!on)}>
      <i />
    </button>
  );
}

export interface Option<T> { value: T; label: string; hint?: string }

function usePopover() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  return { open, setOpen, ref };
}

/** A real dropdown (native <select> looks out of place and can't show hints). */
export function Select<T extends string | number>({ value, options, onChange, width = 230 }: {
  value: T; options: Option<T>[]; onChange: (v: T) => void; width?: number;
}) {
  const { open, setOpen, ref } = usePopover();
  const [hi, setHi] = useState(0);
  const current = options.find((o) => o.value === value);
  const pick = (o: Option<T>) => { onChange(o.value); setOpen(false); };
  return (
    <div className="select" ref={ref} style={{ width }}>
      <button
        type="button"
        className="select-btn"
        onClick={() => { setHi(Math.max(0, options.findIndex((o) => o.value === value))); setOpen(!open); }}
        onKeyDown={(e) => {
          if (!open && (e.key === "ArrowDown" || e.key === "Enter")) { e.preventDefault(); setOpen(true); return; }
          if (!open) return;
          if (e.key === "ArrowDown") { e.preventDefault(); setHi((h) => Math.min(options.length - 1, h + 1)); }
          if (e.key === "ArrowUp") { e.preventDefault(); setHi((h) => Math.max(0, h - 1)); }
          if (e.key === "Enter") { e.preventDefault(); pick(options[hi]); }
          if (e.key === "Escape") setOpen(false);
        }}
      >
        <span>{current?.label ?? "Choose…"}</span>
        <Icon name="chevron" size={12} />
      </button>
      {open && (
        <ul className="menu" role="listbox">
          {options.map((o, i) => (
            <li key={String(o.value)} role="option" aria-selected={o.value === value} className={`${i === hi ? "hi" : ""} ${o.value === value ? "sel" : ""}`}
              onMouseEnter={() => setHi(i)} onMouseDown={(e) => { e.preventDefault(); pick(o); }}>
              <span>{o.label}</span>{o.hint && <small>{o.hint}</small>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * Searchable picker for long lists (AI models). Opens with a search box, filters as you type,
 * arrow keys + Enter pick, and anything typed can be used as-is. `load` runs the first time it opens.
 */
export function Combobox({ value, onChange, load, placeholder = "Choose…", width = 300 }: {
  value: string; onChange: (v: string) => void; load: () => Promise<string[]>; placeholder?: string; width?: number;
}) {
  const { open, setOpen, ref } = usePopover();
  const [items, setItems] = useState<string[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [hi, setHi] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  const fetchItems = () => {
    setErr(null);
    setItems(null);
    load().then(setItems).catch((e) => { setErr(String(e)); setItems([]); });
  };
  useEffect(() => { if (open) { setQ(""); setHi(0); if (items === null) fetchItems(); setTimeout(() => input.current?.focus(), 0); } }, [open]);
  // provider changed: the old list no longer applies
  useEffect(() => setItems(null), [load]);

  const shown = useMemo(() => {
    const list = items ?? [];
    const f = q.trim().toLowerCase();
    const hits = f ? list.filter((m) => m.toLowerCase().includes(f)) : list;
    return q.trim() && !list.includes(q.trim()) ? [...hits, `__custom__${q.trim()}`] : hits;
  }, [items, q]);
  const pick = (v: string) => { onChange(v.replace(/^__custom__/, "")); setOpen(false); };

  return (
    <div className="select combo" ref={ref} style={{ width }}>
      <button type="button" className="select-btn" onClick={() => setOpen(!open)}>
        <span className={value ? "" : "ph"}>{value || placeholder}</span>
        <Icon name="chevron" size={12} />
      </button>
      {open && (
        <div className="menu">
          <div className="search">
            <input
              ref={input}
              value={q}
              placeholder="Search or type a model name"
              onChange={(e) => { setQ(e.target.value); setHi(0); }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") { e.preventDefault(); setHi((h) => Math.min(shown.length - 1, h + 1)); }
                if (e.key === "ArrowUp") { e.preventDefault(); setHi((h) => Math.max(0, h - 1)); }
                if (e.key === "Enter" && shown[hi]) { e.preventDefault(); pick(shown[hi]); }
                if (e.key === "Escape") setOpen(false);
              }}
            />
            <button type="button" className="ghost sm" title="Reload list" onClick={fetchItems}>↻</button>
          </div>
          <ul role="listbox">
            {items === null && <li className="note">Loading models…</li>}
            {err && <li className="note err">{err}</li>}
            {items !== null && !err && shown.length === 0 && <li className="note">No models listed. Type a name to use it.</li>}
            {shown.map((m, i) => (
              <li key={m} className={`${i === hi ? "hi" : ""} ${m === value ? "sel" : ""}`} onMouseEnter={() => setHi(i)}
                onMouseDown={(e) => { e.preventDefault(); pick(m); }}>
                {m.startsWith("__custom__") ? <span>Use “{m.slice(10)}”</span> : <span>{m}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export function Keycap({ children }: { children: ReactNode }) {
  return <kbd className="keycap">{children}</kbd>;
}

/** Shows "Ctrl+Space" as keycaps. */
export function Keys({ combo }: { combo: string }) {
  return <span className="keys">{combo.split("+").map((k) => <Keycap key={k}>{k}</Keycap>)}</span>;
}

/** A settings row: title + description on the left, control on the right. */
export function Row({ title, desc, children }: { title: string; desc?: ReactNode; children?: ReactNode }) {
  return (
    <div className="srow">
      <div className="srow-text"><b>{title}</b>{desc && <span>{desc}</span>}</div>
      <div className="srow-ctl">{children}</div>
    </div>
  );
}

const PATHS: Record<string, string> = {
  home: "M4 10.5 12 4l8 6.5V19a1.5 1.5 0 0 1-1.5 1.5H15v-6h-6v6H5.5A1.5 1.5 0 0 1 4 19z",
  history: "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17zM12 7.5V12l3 2",
  settings: "M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6zM12 2.8l1.6 2.4 2.8-.7.8 2.8 2.7 1-1 2.7 1.8 2.2-2.2 1.8.4 2.9-2.9.3-1.3 2.6L12 19.4l-2.5 1.4-1.3-2.6-2.9-.3.4-2.9L3.5 13.2l1.8-2.2-1-2.7 2.7-1 .8-2.8 2.8.7z",
  mic: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21",
  lock: "M7 10.5h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 10.5V8a4 4 0 0 1 8 0v2.5",
  chevron: "M7 10l5 5 5-5",
  models: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12 4 7.5",
  words: "M5 6h14M5 12h10M5 18h7",
  apps: "M5 5h5v5H5zM14 5h5v5h-5zM5 14h5v5H5zM14 14h5v5h-5z",
  ai: "M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z",
  fast: "M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15zM12 12l3-2.5",
  minimal: "M7.5 4.5h9a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3v-9a3 3 0 0 1 3-3zM10 10h4v4h-4z",
  copy: "M9 9h10v10H9zM5 15V5h10",
  trash: "M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12",
  check: "M5 12.5l4.5 4.5L19 7.5",
  download: "M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19.5h14",
};

export function Icon({ name, size = 16 }: { name: keyof typeof PATHS | string; size?: number }) {
  return (
    <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={PATHS[name] ?? ""} />
    </svg>
  );
}
