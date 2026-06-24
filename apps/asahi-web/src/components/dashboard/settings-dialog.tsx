import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ExternalLink, X } from "lucide-react";

const CLOSE_MS = 120;

/**
 * Settings dialog. Reuses .asahi-modal-* from style.css for animation parity
 * with the New issue dialog. Local mount/unmount + state so the exit transition
 * plays before the element is removed.
 *
 * Content is intentionally sparse: identity, the two real toggles we expose
 * today (theme, motion are read-only previews until they're wired), and the
 * daemon facts pulled from WORKFLOW.md. Sections that were filler in the
 * earlier version (Account, About) have been removed.
 */
export function SettingsDialog({ onClose, open }: { onClose: () => void; open: boolean }) {
  const [mounted, setMounted] = useState(false);
  const [animState, setAnimState] = useState<"open" | "closing">("open");
  const closeTimer = useRef<number | null>(null);

  useEffect(() => {
    if (open) {
      if (closeTimer.current != null) {
        window.clearTimeout(closeTimer.current);
        closeTimer.current = null;
      }
      setAnimState("open");
      setMounted(true);
      return;
    }
    if (!mounted) return;
    setAnimState("closing");
    closeTimer.current = window.setTimeout(() => {
      setMounted(false);
      closeTimer.current = null;
    }, CLOSE_MS);
  }, [open, mounted]);

  useEffect(() => {
    return () => {
      if (closeTimer.current != null) window.clearTimeout(closeTimer.current);
    };
  }, []);

  const [entered, setEntered] = useState(false);
  useEffect(() => {
    if (!mounted) {
      setEntered(false);
      return;
    }
    const id = requestAnimationFrame(() => setEntered(true));
    return () => cancelAnimationFrame(id);
  }, [mounted]);

  const state: "open" | "closing" | undefined =
    animState === "closing" ? "closing" : entered ? "open" : undefined;

  // Latest-ref pattern: keep onClose accessible to the effect without
  // tearing down the document listener every render. Equivalent to React's
  // experimental useEffectEvent without the experimental import.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useEffect(() => {
    if (!mounted) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [mounted]);

  if (!mounted) return null;

  const node = (
    <div
      className="asahi-modal-backdrop fixed inset-0 z-50 flex items-start justify-center bg-black/24 px-4 pt-[14vh] backdrop-blur-[1px]"
      data-state={state}
    >
      <button
        aria-label="Close settings"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <div
        aria-labelledby="settings-title"
        className="asahi-modal-panel relative flex max-h-[min(32rem,calc(100svh-8rem))] w-[min(28rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-xl bg-card text-card-foreground ring-1 ring-border/70"
        data-state={state}
        role="dialog"
      >
        <header className="flex items-start justify-between gap-3 px-5 pb-4 pt-4">
          <div className="flex items-center gap-3">
            <span className="inline-flex size-7 items-center justify-center rounded-full bg-foreground text-[11px] font-medium text-background">
              L
            </span>
            <div className="flex min-w-0 flex-col">
              <span className="text-[13.5px] font-medium text-foreground" id="settings-title">
                Luna
              </span>
              <span className="text-[11.5px] text-muted-foreground">Local daemon</span>
            </div>
          </div>
          <button
            aria-label="Close settings"
            className="asahi-press inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={onClose}
            type="button"
          >
            <X className="size-4" />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto">
          <Section eyebrow="Appearance">
            <Row label="Theme">
              <ValueChip>Light</ValueChip>
              <Footnote>Dark mode is coming.</Footnote>
            </Row>
          </Section>

          <Section eyebrow="Daemon">
            <Row label="Tracker">
              <KeyValue k="kind" v="asahi" />
              <KeyValue k="db" v="./asahi.db" />
              <KeyValue k="port" v="49305" />
            </Row>
            <Row label="Polling">
              <KeyValue k="interval_ms" v="30000" />
            </Row>
            <Row label="Scheduler">
              <KeyValue k="max_concurrent" v="4" />
              <KeyValue k="max_turns" v="20" />
              <KeyValue k="retry_backoff_ms" v="300000" />
            </Row>
            <Footnote>Edit values in WORKFLOW.md and restart Luna to apply.</Footnote>
          </Section>

          <Section eyebrow="About">
            <Row label="Asahi">
              <ValueChip>The tracker that powers Luna.</ValueChip>
            </Row>
            <Row label="Version">
              <span className="font-mono text-[12px] text-foreground">0.0.0</span>
            </Row>
            <Row label="Links">
              <a
                className="inline-flex items-center gap-1.5 text-[13px] text-foreground hover:underline"
                href="https://github.com/AkaraChen/luna"
                rel="noreferrer"
                target="_blank"
              >
                GitHub <ExternalLink className="size-3" />
              </a>
              <a
                className="inline-flex items-center gap-1.5 text-[13px] text-foreground hover:underline"
                href="https://github.com/AkaraChen/luna/blob/HEAD/README.md"
                rel="noreferrer"
                target="_blank"
              >
                README <ExternalLink className="size-3" />
              </a>
            </Row>
          </Section>
        </div>
      </div>
    </div>
  );

  return createPortal(node, document.body);
}

function Section({ children, eyebrow }: { children: React.ReactNode; eyebrow: string }) {
  return (
    <section className="border-t border-border/60 px-5 py-4 first:border-t-0">
      <h3 className="asahi-eyebrow mb-3">{eyebrow}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function Row({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <div className="grid grid-cols-[5.5rem_minmax(0,1fr)] items-baseline gap-x-4">
      <div className="text-[12px] text-muted-foreground">{label}</div>
      <div className="flex flex-col items-start gap-1">{children}</div>
    </div>
  );
}

function ValueChip({ children }: { children: React.ReactNode }) {
  return <span className="text-[13px] text-foreground">{children}</span>;
}

function KeyValue({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex items-baseline gap-3 text-[12px]">
      <span className="font-mono text-muted-foreground">{k}</span>
      <span className="font-mono text-foreground">{v}</span>
    </div>
  );
}

function Footnote({ children }: { children: React.ReactNode }) {
  return <p className="text-[11.5px] text-muted-foreground">{children}</p>;
}
