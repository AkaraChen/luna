import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Suspense, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type AsahiNotification } from "@/api/asahi";

import { NotificationsView } from "./notifications-view";

const requests: Array<{ method: string; path: string }> = [];

function notification(overrides: Partial<AsahiNotification>): AsahiNotification {
  return {
    id: "notification-1",
    type: "comment",
    issue_id: "issue-1",
    issue: {
      id: "issue-1",
      identifier: "ASAHI-1",
      title: "Wire permissions",
      state: "Todo",
      priority: 1,
      updated_at: "2026-01-01T00:00:00Z",
    },
    recipient_id: null,
    actor_id: null,
    title: "New comment",
    body: "Please check this",
    read_at: null,
    archived_at: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function renderWithClient(element: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <Suspense fallback={<div>Loading</div>}>{element}</Suspense>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  requests.length = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = input instanceof Request ? input.url : String(input);
      const method = init?.method ?? "GET";
      requests.push({ method, path });

      if (path.startsWith("/api/notifications")) {
        return Response.json({
          notifications: [
            notification({ id: "notification-1", read_at: null }),
            notification({
              id: "notification-2",
              issue_id: "issue-2",
              issue: {
                id: "issue-2",
                identifier: "ASAHI-2",
                title: "Batch comment polling",
                state: "In Progress",
                priority: 2,
                updated_at: "2026-01-02T00:00:00Z",
              },
              title: "Agent update",
              body: "Done",
              read_at: "2026-01-02T00:00:00Z",
            }),
          ],
          unread_count: 1,
        });
      }

      return Response.json(notification({ id: "updated" }));
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("NotificationsView", () => {
  it("renders notifications and unread state", async () => {
    renderWithClient(<NotificationsView />);

    expect(await screen.findByText("New comment")).toBeInTheDocument();
    expect(screen.getByText("Wire permissions")).toBeInTheDocument();
    expect(screen.getByText("Agent update")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Unread" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Unread1/ })).toBeInTheDocument();
  });

  it("fires read and archive API calls", async () => {
    renderWithClient(<NotificationsView />);
    await screen.findByText("New comment");

    fireEvent.click(screen.getByRole("button", { name: /Mark all read/ }));
    await waitFor(() =>
      expect(requests).toContainEqual({
        method: "PATCH",
        path: "/api/notifications/notification-1/read",
      }),
    );

    fireEvent.click(screen.getAllByLabelText("Archive notification")[0]);
    await waitFor(() =>
      expect(requests).toContainEqual({
        method: "PATCH",
        path: "/api/notifications/notification-1/archive",
      }),
    );
  });
});
