import { useEffect, useRef, useState } from "react";

import { IssueComposer } from "./issue-composer";

// Match the exit transition duration declared on .asahi-modal-* in style.css.
// Keep these in sync.
const CLOSE_MS = 120;

export function CreateIssueTrigger({
  children,
  projectId,
}: {
  children: React.ReactNode;
  projectId?: string;
}) {
  const [mounted, setMounted] = useState(false);
  const [state, setState] = useState<"open" | "closing">("open");
  const closeTimer = useRef<number | null>(null);

  const open = () => {
    if (closeTimer.current != null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    setState("open");
    setMounted(true);
  };

  const requestClose = () => {
    setState("closing");
    closeTimer.current = window.setTimeout(() => {
      setMounted(false);
      closeTimer.current = null;
    }, CLOSE_MS);
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "t") {
        event.preventDefault();
        open();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  useEffect(() => {
    return () => {
      if (closeTimer.current != null) {
        window.clearTimeout(closeTimer.current);
      }
    };
  }, []);

  return (
    <>
      <span className="inline-flex items-center" onClick={open}>
        {children}
      </span>
      {mounted ? (
        <IssueComposer
          dataState={state}
          onClose={requestClose}
          projectId={projectId}
        />
      ) : null}
    </>
  );
}
