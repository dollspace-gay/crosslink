// Fetch wrapper + React Query hooks for the /api/v1/dashboard endpoints.
//
// Bearer-token auth is installed globally by `auth/bootstrap.ts` before
// React mounts (it wraps `globalThis.fetch`), so these helpers can use
// the bare `fetch` API without re-plumbing headers.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { AlertItem, ProjectDetail, ProjectListItem } from "./types";

const API_BASE = "/api/v1/dashboard";

/// Default refetch cadence. Matches the server-side poll loop
/// (`crosslink/src/dashboard/poll.rs::DEFAULT_TICK = 5s`) so the
/// frontend's view stays within one tick of the ground truth.
const REFETCH_MS = 5_000;

export class ApiRequestError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiRequestError";
  }
}

async function apiFetch<T>(path: string): Promise<T> {
  const resp = await fetch(`${API_BASE}${path}`, {
    headers: { Accept: "application/json" },
  });
  if (!resp.ok) {
    let message = `HTTP ${resp.status}`;
    try {
      const body = (await resp.json()) as { error?: string };
      if (body.error) message = body.error;
    } catch {
      // Non-JSON error body; fall back to status-only message.
    }
    throw new ApiRequestError(resp.status, message);
  }
  return (await resp.json()) as T;
}

async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const resp = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!resp.ok) {
    let message = `HTTP ${resp.status}`;
    try {
      const parsed = (await resp.json()) as { error?: string };
      if (parsed.error) message = parsed.error;
    } catch {
      // Non-JSON error body.
    }
    throw new ApiRequestError(resp.status, message);
  }
  // Responses may be empty; guard against that.
  const text = await resp.text();
  return (text ? JSON.parse(text) : ({} as T)) as T;
}

export interface ActionResponse {
  stdout: string;
  stderr: string;
}

/// `useQuery` hook for the project-list endpoint. Polls every 5s so
/// tiles stay current without requiring the WebSocket upgrade
/// (which lands in P1.5).
export function useProjects() {
  return useQuery<ProjectListItem[], ApiRequestError>({
    queryKey: ["dashboard", "projects"],
    queryFn: () => apiFetch<ProjectListItem[]>("/projects"),
    refetchInterval: REFETCH_MS,
    refetchIntervalInBackground: false,
  });
}

/// Detail hook. `slug` is `owner/repo` — the wildcard route handles
/// the embedded slash server-side. `null` slug disables the query
/// (useful when the route param isn't resolved yet).
export function useProject(slug: string | null) {
  return useQuery<ProjectDetail, ApiRequestError>({
    queryKey: ["dashboard", "project", slug],
    queryFn: () => apiFetch<ProjectDetail>(`/projects/${slug}`),
    refetchInterval: REFETCH_MS,
    refetchIntervalInBackground: false,
    enabled: slug !== null,
  });
}

/// Currently-open alerts across all projects. Primary use case is
/// the alert rail in the header and the `/alerts` page. WS events
/// invalidate this cache on every `dashboard_alerts_changed` tick.
export function useAlerts() {
  return useQuery<AlertItem[], ApiRequestError>({
    queryKey: ["dashboard", "alerts"],
    queryFn: () => apiFetch<AlertItem[]>("/alerts"),
    refetchInterval: REFETCH_MS,
    refetchIntervalInBackground: false,
  });
}

/// Shared post-mutation invalidator. Both the project detail query
/// and the global projects query get invalidated so the tile grid
/// and drill-down page catch up to the new state. The next polling
/// tick would do this anyway, but optimistic invalidation makes the
/// click-to-visible latency feel snappy.
function useProjectMutations(slug: string) {
  const client = useQueryClient();
  return (after: () => void = () => undefined) => {
    client.invalidateQueries({ queryKey: ["dashboard", "projects"] });
    client.invalidateQueries({ queryKey: ["dashboard", "project", slug] });
    after();
  };
}

/// Close an issue via the dashboard's write surface. Under the hood
/// this shells out to `crosslink issue close <id>` in the tracked
/// project's workspace — identity, signing, and hub push all handled
/// by the user's normal crosslink setup.
export function useCloseIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<ActionResponse, ApiRequestError, number>({
    mutationFn: (issueId: number) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/close`),
    onSuccess: () => invalidate(),
  });
}

/// Reopen a closed issue.
export function useReopenIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<ActionResponse, ApiRequestError, number>({
    mutationFn: (issueId: number) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/reopen`),
    onSuccess: () => invalidate(),
  });
}

/// Post a comment on an issue. `content` goes through to the CLI's
/// `crosslink issue comment <id> "<content>"` — whitespace-only
/// content is rejected server-side with a 400.
export function useCommentIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; content: string }
  >({
    mutationFn: ({ issueId, content }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/comment`, {
        content,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Mark issue `issueId` as blocked by `blockerId`.
export function useBlockIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; blockerId: number }
  >({
    mutationFn: ({ issueId, blockerId }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/block`, {
        blocker_id: blockerId,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Drop a blocker relationship.
export function useUnblockIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; blockerId: number }
  >({
    mutationFn: ({ issueId, blockerId }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/unblock`, {
        blocker_id: blockerId,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Symmetric link between two issues.
export function useRelateIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; otherId: number }
  >({
    mutationFn: ({ issueId, otherId }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/relate`, {
        other_id: otherId,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Add a label to an issue.
export function useLabelIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; label: string }
  >({
    mutationFn: ({ issueId, label }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/label`, {
        label,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Remove a label from an issue.
export function useUnlabelIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; label: string }
  >({
    mutationFn: ({ issueId, label }) =>
      apiPost<ActionResponse>(`/w/${slug}/issues/${issueId}/unlabel`, {
        label,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Create a new milestone. `description` is optional.
export function useCreateMilestone(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { name: string; description?: string }
  >({
    mutationFn: ({ name, description }) =>
      apiPost<ActionResponse>(`/w/${slug}/milestones`, {
        name,
        description,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Attach issue to milestone.
export function useMilestoneAddIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { milestoneId: number; issueId: number }
  >({
    mutationFn: ({ milestoneId, issueId }) =>
      apiPost<ActionResponse>(`/w/${slug}/milestones/${milestoneId}/add`, {
        issue_id: issueId,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Detach issue from milestone.
export function useMilestoneRemoveIssue(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { milestoneId: number; issueId: number }
  >({
    mutationFn: ({ milestoneId, issueId }) =>
      apiPost<ActionResponse>(`/w/${slug}/milestones/${milestoneId}/remove`, {
        issue_id: issueId,
      }),
    onSuccess: () => invalidate(),
  });
}

/// Close a milestone (sets status to closed; does not delete).
export function useCloseMilestone(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<ActionResponse, ApiRequestError, number>({
    mutationFn: (milestoneId: number) =>
      apiPost<ActionResponse>(`/w/${slug}/milestones/${milestoneId}/close`),
    onSuccess: () => invalidate(),
  });
}

/// Claim a lock on an issue. `branch` is optional context metadata.
/// Operator-initiated claims are uncommon — normally agents claim
/// their own locks — but exposing the control lets an operator seed
/// a lock during triage.
export function useClaimLock(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<
    ActionResponse,
    ApiRequestError,
    { issueId: number; branch?: string }
  >({
    mutationFn: ({ issueId, branch }) =>
      apiPost<ActionResponse>(`/w/${slug}/locks/${issueId}/claim`, { branch }),
    onSuccess: () => invalidate(),
  });
}

/// Release a lock held by the current driver.
export function useReleaseLock(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<ActionResponse, ApiRequestError, number>({
    mutationFn: (issueId: number) =>
      apiPost<ActionResponse>(`/w/${slug}/locks/${issueId}/release`),
    onSuccess: () => invalidate(),
  });
}

/// Steal a stale lock from another agent. The CLI enforces the
/// staleness threshold — this endpoint just passes through.
export function useStealLock(slug: string) {
  const invalidate = useProjectMutations(slug);
  return useMutation<ActionResponse, ApiRequestError, number>({
    mutationFn: (issueId: number) =>
      apiPost<ActionResponse>(`/w/${slug}/locks/${issueId}/steal`),
    onSuccess: () => invalidate(),
  });
}
