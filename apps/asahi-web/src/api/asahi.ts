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

export interface IssueListResponse {
  issues: Issue[];
}

export interface CommentListResponse {
  comments: Comment[];
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
