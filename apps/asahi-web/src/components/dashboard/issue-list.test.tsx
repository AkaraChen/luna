import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { type Issue } from "@/api/asahi";

import { IssueList } from "./issue-list";

function issue(overrides: Partial<Issue>): Issue {
  return {
    id: "issue-1",
    identifier: "ASAHI-1",
    project_id: null,
    project: null,
    title: "Default issue",
    description: null,
    priority: 1,
    state: "Todo",
    branch_name: null,
    url: null,
    labels: [],
    blocked_by: [],
    created_at: null,
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("IssueList", () => {
  it("renders issue identifiers and titles", () => {
    render(
      <IssueList
        issues={[
          issue({ id: "issue-1", identifier: "ASAHI-1", title: "Wire permissions" }),
          issue({
            id: "issue-2",
            identifier: "ASAHI-2",
            title: "Batch comment polling",
            state: "In Progress",
            priority: 2,
          }),
        ]}
        onSelect={() => {}}
        selectedId={null}
      />,
    );

    expect(screen.getByText("ASAHI-1")).toBeInTheDocument();
    expect(screen.getByText("Wire permissions")).toBeInTheDocument();
    expect(screen.getByText("ASAHI-2")).toBeInTheDocument();
    expect(screen.getByText("Batch comment polling")).toBeInTheDocument();
  });

  it("renders the empty state", () => {
    render(<IssueList issues={[]} onSelect={() => {}} selectedId={null} />);

    expect(screen.getByText("No issues")).toBeInTheDocument();
    expect(screen.getByText("Try a different status or search.")).toBeInTheDocument();
  });

  it("calls onSelect with the selected issue id", () => {
    const onSelect = vi.fn();
    render(
      <IssueList
        issues={[issue({ id: "issue-2", identifier: "ASAHI-2", title: "Clickable issue" })]}
        onSelect={onSelect}
        selectedId={null}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Open issue ASAHI-2/ }));

    expect(onSelect).toHaveBeenCalledWith("issue-2");
  });
});
