import { useEffect, useState } from "react";

import { IssueComposer } from "./issue-composer";

export function CreateIssueTrigger({
  children,
  projectId,
}: {
  children: React.ReactNode;
  projectId?: string;
}) {
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
      <span
        className="inline-flex items-center"
        onClick={() => setOpen(true)}
      >
        {children}
      </span>
      {open ? <IssueComposer onClose={() => setOpen(false)} projectId={projectId} /> : null}
    </>
  );
}
