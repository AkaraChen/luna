export interface BlockerRef {
  id: string | null;
  identifier: string | null;
  state: string | null;
}

export interface Issue {
  id: string;
  identifier: string;
  title: string;
  description: string | null;
  priority: number | null;
  state: string;
  branch_name: string | null;
  url: string | null;
  labels: string[];
  blocked_by: BlockerRef[];
  created_at: string | null;
  updated_at: string | null;
}

export interface Comment {
  id: string;
  issue_id: string;
  body: string;
  created_at: string;
}

export interface NotificationIssueRef {
  id: string;
  identifier: string;
  title: string;
  state: string;
  priority: number | null;
  updated_at: string | null;
}

export interface AsahiNotification {
  id: string;
  type: string;
  issue_id: string | null;
  issue: NotificationIssueRef | null;
  recipient_id: string | null;
  actor_id: string | null;
  title: string;
  body: string | null;
  read_at: string | null;
  archived_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface IssueListResponse {
  issues: Issue[];
}

export interface CommentListResponse {
  comments: Comment[];
}

export interface NotificationListResponse {
  notifications: AsahiNotification[];
  unread_count: number;
}

export interface CreateIssueInput {
  project_slug?: string;
  team_key?: string;
  title: string;
  description?: string;
  priority?: number;
  state?: string;
  branch_name?: string;
  labels?: string[];
  blocked_by?: string[];
  assignee_id?: string;
}

export interface UpdateIssueInput {
  priority?: number | null;
  blocked_by?: string[];
}

export async function fetchIssues(
  options: {
    states?: string[];
  } = {},
): Promise<IssueListResponse> {
  const params = new URLSearchParams();
  if (options.states?.length) {
    params.set("states", options.states.join(","));
  }
  return request<IssueListResponse>(`/api/issues${queryString(params)}`);
}

export async function createIssue(input: CreateIssueInput): Promise<Issue> {
  return request<Issue>("/api/issues", {
    body: JSON.stringify(input),
    method: "POST",
  });
}

export async function updateIssue(issueId: string, input: UpdateIssueInput): Promise<Issue> {
  return request<Issue>(`/api/issues/${encodeURIComponent(issueId)}`, {
    body: JSON.stringify(input),
    method: "PATCH",
  });
}

export async function deleteIssue(issueId: string): Promise<Issue> {
  return request<Issue>(`/api/issues/${encodeURIComponent(issueId)}`, {
    method: "DELETE",
  });
}

export async function updateIssueState(issueId: string, state: string): Promise<Issue> {
  return request<Issue>(`/api/issues/${encodeURIComponent(issueId)}/state`, {
    body: JSON.stringify({ state }),
    method: "PATCH",
  });
}

export async function fetchComments(issueId: string): Promise<CommentListResponse> {
  return request<CommentListResponse>(`/api/issues/${encodeURIComponent(issueId)}/comments`);
}

export async function createComment(issueId: string, body: string): Promise<Comment> {
  return request<Comment>(`/api/issues/${encodeURIComponent(issueId)}/comments`, {
    body: JSON.stringify({ body }),
    method: "POST",
  });
}

export async function fetchNotifications(
  options: {
    include_archived?: boolean;
    unread_only?: boolean;
    recipient_id?: string;
    issue_id?: string;
    limit?: number;
  } = {},
): Promise<NotificationListResponse> {
  const params = new URLSearchParams();
  if (options.include_archived != null) {
    params.set("include_archived", String(options.include_archived));
  }
  if (options.unread_only != null) {
    params.set("unread_only", String(options.unread_only));
  }
  if (options.recipient_id) {
    params.set("recipient_id", options.recipient_id);
  }
  if (options.issue_id) {
    params.set("issue_id", options.issue_id);
  }
  if (options.limit != null) {
    params.set("limit", String(options.limit));
  }
  return request<NotificationListResponse>(`/api/notifications${queryString(params)}`);
}

export async function markNotificationRead(notificationId: string): Promise<AsahiNotification> {
  return request<AsahiNotification>(
    `/api/notifications/${encodeURIComponent(notificationId)}/read`,
    { method: "PATCH" },
  );
}

export async function archiveNotification(notificationId: string): Promise<AsahiNotification> {
  return request<AsahiNotification>(
    `/api/notifications/${encodeURIComponent(notificationId)}/archive`,
    { method: "PATCH" },
  );
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(path, {
    ...init,
    headers,
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(body || `Request failed: ${response.status}`);
  }

  return response.json() as Promise<T>;
}

function queryString(params: URLSearchParams) {
  const value = params.toString();
  return value ? `?${value}` : "";
}
