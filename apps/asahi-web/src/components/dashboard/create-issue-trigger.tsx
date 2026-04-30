import { useEffect, useState } from "react";

import { IssueComposer } from "./issue-composer";

export function CreateIssueTrigger({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "t") {
        event.preventDefault();
        setOpen(true);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  return (
    <>
      <button
        className="inline-flex items-center"
        onClick={() => setOpen(true)}
        type="button"
      >
        {children}
      </button>
      {open ? <IssueComposer onClose={() => setOpen(false)} /> : null}
    </>
  );
}
