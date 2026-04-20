// Drill-down view for a single tracked project. Pulls the full
// `ProjectDetail` payload from `/api/v1/dashboard/projects/{*slug}` and
// renders sections for issues, agents, and locks. Includes the full
// P1.8–P1.9 write surface: close / reopen / comment / block / unblock /
// relate / label / unlabel on issues, plus create-milestone.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import {
  useBlockIssue,
  useCloseIssue,
  useCommentIssue,
  useCreateMilestone,
  useLabelIssue,
  useProject,
  useRelateIssue,
  useReleaseLock,
  useReopenIssue,
  useStealLock,
  useUnblockIssue,
  useUnlabelIssue,
} from "@/api/client";
import type { IssueFile, LockEntry } from "@/api/types";

export function ProjectDetail() {
  // React Router wildcards ({*slug}) are surfaced via the `"*"` param key.
  const { "*": slug } = useParams();
  const { data, isLoading, error } = useProject(slug ?? null);

  if (!slug) {
    return <FallbackMessage tone="error">Missing project slug in URL.</FallbackMessage>;
  }
  if (isLoading) {
    return <FallbackMessage tone="info">Loading {slug}…</FallbackMessage>;
  }
  if (error) {
    return (
      <FallbackMessage tone="error">
        Failed to load {slug}: {error.message}
      </FallbackMessage>
    );
  }
  if (!data) {
    return <FallbackMessage tone="info">No data for {slug}.</FallbackMessage>;
  }

  const openIssues = data.issues.filter((i) => i.status === "open");
  const closedIssues = data.issues.filter((i) => i.status === "closed");

  return (
    <main className="mx-auto max-w-6xl px-6 py-6">
      <nav className="mb-4 text-sm">
        <Link to="/" className="text-muted-foreground hover:underline">
          ← All projects
        </Link>
      </nav>
      <header className="mb-6 flex items-baseline justify-between">
        <h1 className="text-2xl font-semibold">{data.slug}</h1>
        <span className="text-xs text-muted-foreground">
          layout v{data.layout_version}
          {data.hub_sha && ` · ${data.hub_sha.slice(0, 7)}`}
        </span>
      </header>

      <section className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-6">
        <Counter label="Open" value={data.counters.open_issues} />
        <Counter label="Overdue" value={data.counters.overdue_issues} warn={data.counters.overdue_issues > 0} />
        <Counter label="Due soon" value={data.counters.due_soon_issues} />
        <Counter label="Blocked" value={data.counters.blocked_issues} warn={data.counters.blocked_issues > 0} />
        <Counter label="Agents" value={data.counters.active_agents} />
        <Counter label="Stale locks" value={data.counters.stale_locks} warn={data.counters.stale_locks > 0} />
      </section>

      <section className="mb-8">
        <h2 className="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Agents ({data.agents.length})
        </h2>
        {data.agents.length === 0 ? (
          <p className="text-sm text-muted-foreground">No agents have heartbeated on this project.</p>
        ) : (
          <ul className="divide-y divide-border rounded border bg-card">
            {data.agents.map((a) => (
              <li key={a.agent_id} className="flex items-baseline justify-between px-3 py-2 text-sm">
                <span className="font-medium">{a.agent_id}</span>
                <span className="text-xs text-muted-foreground tabular-nums">
                  last heartbeat {a.last_heartbeat}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="mb-8">
        <h2 className="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Locks ({data.locks.length})
        </h2>
        {data.locks.length === 0 ? (
          <p className="text-sm text-muted-foreground">No active locks.</p>
        ) : (
          <ul className="divide-y divide-border rounded border bg-card">
            {data.locks.map((l) => (
              <LockRow key={l.issue_id} slug={data.slug} lock={l} />
            ))}
          </ul>
        )}
      </section>

      <section className="mb-6">
        <h2 className="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Milestones
        </h2>
        <NewMilestoneForm slug={data.slug} />
      </section>

      <section>
        <h2 className="mb-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Issues ({openIssues.length} open, {closedIssues.length} closed)
        </h2>
        {openIssues.length === 0 ? (
          <p className="text-sm text-muted-foreground">No open issues.</p>
        ) : (
          <ul className="divide-y divide-border rounded border bg-card">
            {openIssues.map((i) => (
              <OpenIssueRow key={i.uuid} slug={data.slug} issue={i} />
            ))}
          </ul>
        )}
        {closedIssues.length > 0 && (
          <details className="mt-4">
            <summary className="cursor-pointer text-xs uppercase tracking-wide text-muted-foreground">
              Closed issues ({closedIssues.length})
            </summary>
            <ul className="mt-2 divide-y divide-border rounded border bg-card">
              {closedIssues.map((i) => (
                <ClosedIssueRow key={i.uuid} slug={data.slug} issue={i} />
              ))}
            </ul>
          </details>
        )}
      </section>
    </main>
  );
}

function OpenIssueRow({ slug, issue }: { slug: string; issue: IssueFile }) {
  const [commentOpen, setCommentOpen] = useState(false);
  const [commentText, setCommentText] = useState("");
  const [moreOpen, setMoreOpen] = useState(false);
  const [labelText, setLabelText] = useState("");
  const [blockerText, setBlockerText] = useState("");
  const [unblockText, setUnblockText] = useState("");
  const [relateText, setRelateText] = useState("");

  const close = useCloseIssue(slug);
  const comment = useCommentIssue(slug);
  const label = useLabelIssue(slug);
  const unlabel = useUnlabelIssue(slug);
  const block = useBlockIssue(slug);
  const unblock = useUnblockIssue(slug);
  const relate = useRelateIssue(slug);

  const id = issue.display_id;
  const canAct = id != null;

  const moreError =
    label.error?.message ??
    unlabel.error?.message ??
    block.error?.message ??
    unblock.error?.message ??
    relate.error?.message;

  return (
    <li className="px-3 py-2 text-sm">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="text-muted-foreground tabular-nums">
          {id != null ? `#${id}` : "—"}
        </span>
        <span className="font-medium">{issue.title}</span>
        <span className="text-xs text-muted-foreground">[{issue.priority}]</span>
        {issue.due_at && (
          <span className="text-xs text-muted-foreground">due {issue.due_at}</span>
        )}
        {issue.blockers.length > 0 && (
          <span className="text-xs text-amber-500">
            blocked by {issue.blockers.length}
          </span>
        )}
        <span className="ml-auto flex items-center gap-2">
          <button
            type="button"
            disabled={!canAct || close.isPending}
            onClick={() => (id != null ? close.mutate(id) : undefined)}
            className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
          >
            {close.isPending ? "Closing…" : "Close"}
          </button>
          <button
            type="button"
            disabled={!canAct}
            onClick={() => setCommentOpen((v) => !v)}
            className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
          >
            {commentOpen ? "Cancel" : "Comment"}
          </button>
          <button
            type="button"
            disabled={!canAct}
            onClick={() => setMoreOpen((v) => !v)}
            className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
          >
            {moreOpen ? "Hide" : "More"}
          </button>
        </span>
      </div>
      {issue.labels.length > 0 && (
        <div className="mt-1 flex flex-wrap gap-1">
          {issue.labels.map((l) => (
            <span
              key={l}
              className="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground"
            >
              {l}
              {canAct && id != null && (
                <button
                  type="button"
                  disabled={unlabel.isPending}
                  onClick={() =>
                    unlabel.mutate({ issueId: id, label: l })
                  }
                  className="text-muted-foreground hover:text-rose-500 disabled:opacity-50"
                  aria-label={`Remove label ${l}`}
                >
                  ×
                </button>
              )}
            </span>
          ))}
        </div>
      )}
      {close.error && (
        <p className="mt-1 text-xs text-rose-500">{close.error.message}</p>
      )}
      {commentOpen && canAct && id != null && (
        <form
          className="mt-2 flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!commentText.trim()) return;
            comment.mutate(
              { issueId: id, content: commentText },
              {
                onSuccess: () => {
                  setCommentText("");
                  setCommentOpen(false);
                },
              },
            );
          }}
        >
          <textarea
            value={commentText}
            onChange={(e) => setCommentText(e.target.value)}
            placeholder="Comment text"
            rows={3}
            className="w-full rounded border bg-background p-2 text-sm"
          />
          <div className="flex items-center gap-2">
            <button
              type="submit"
              disabled={!commentText.trim() || comment.isPending}
              className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
            >
              {comment.isPending ? "Posting…" : "Post comment"}
            </button>
            {comment.error && (
              <span className="text-xs text-rose-500">{comment.error.message}</span>
            )}
          </div>
        </form>
      )}
      {moreOpen && canAct && id != null && (
        <div className="mt-2 flex flex-col gap-2 rounded border bg-background/50 p-2">
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const v = labelText.trim();
              if (!v) return;
              label.mutate(
                { issueId: id, label: v },
                { onSuccess: () => setLabelText("") },
              );
            }}
          >
            <label className="text-xs text-muted-foreground w-16">Label</label>
            <input
              value={labelText}
              onChange={(e) => setLabelText(e.target.value)}
              placeholder="label-name"
              className="flex-1 rounded border bg-background px-2 py-0.5 text-xs"
            />
            <button
              type="submit"
              disabled={!labelText.trim() || label.isPending}
              className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
            >
              {label.isPending ? "Adding…" : "Add"}
            </button>
          </form>
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const n = Number(blockerText.trim());
              if (!Number.isInteger(n) || n <= 0) return;
              block.mutate(
                { issueId: id, blockerId: n },
                { onSuccess: () => setBlockerText("") },
              );
            }}
          >
            <label className="text-xs text-muted-foreground w-16">
              Blocked by
            </label>
            <input
              type="number"
              min={1}
              value={blockerText}
              onChange={(e) => setBlockerText(e.target.value)}
              placeholder="issue id"
              className="flex-1 rounded border bg-background px-2 py-0.5 text-xs"
            />
            <button
              type="submit"
              disabled={!blockerText.trim() || block.isPending}
              className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
            >
              {block.isPending ? "Adding…" : "Block"}
            </button>
          </form>
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const n = Number(unblockText.trim());
              if (!Number.isInteger(n) || n <= 0) return;
              unblock.mutate(
                { issueId: id, blockerId: n },
                { onSuccess: () => setUnblockText("") },
              );
            }}
          >
            <label className="text-xs text-muted-foreground w-16">Unblock</label>
            <input
              type="number"
              min={1}
              value={unblockText}
              onChange={(e) => setUnblockText(e.target.value)}
              placeholder="issue id"
              className="flex-1 rounded border bg-background px-2 py-0.5 text-xs"
            />
            <button
              type="submit"
              disabled={!unblockText.trim() || unblock.isPending}
              className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
            >
              {unblock.isPending ? "Clearing…" : "Clear"}
            </button>
          </form>
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const n = Number(relateText.trim());
              if (!Number.isInteger(n) || n <= 0) return;
              relate.mutate(
                { issueId: id, otherId: n },
                { onSuccess: () => setRelateText("") },
              );
            }}
          >
            <label className="text-xs text-muted-foreground w-16">
              Related
            </label>
            <input
              type="number"
              min={1}
              value={relateText}
              onChange={(e) => setRelateText(e.target.value)}
              placeholder="issue id"
              className="flex-1 rounded border bg-background px-2 py-0.5 text-xs"
            />
            <button
              type="submit"
              disabled={!relateText.trim() || relate.isPending}
              className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
            >
              {relate.isPending ? "Linking…" : "Link"}
            </button>
          </form>
          {moreError && (
            <p className="text-xs text-rose-500">{moreError}</p>
          )}
        </div>
      )}
    </li>
  );
}

function NewMilestoneForm({ slug }: { slug: string }) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [expanded, setExpanded] = useState(false);
  const create = useCreateMilestone(slug);

  if (!expanded) {
    return (
      <button
        type="button"
        onClick={() => setExpanded(true)}
        className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10"
      >
        + New milestone
      </button>
    );
  }

  return (
    <form
      className="flex flex-col gap-2 rounded border bg-card p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!name.trim()) return;
        create.mutate(
          {
            name: name.trim(),
            description: description.trim() || undefined,
          },
          {
            onSuccess: () => {
              setName("");
              setDescription("");
              setExpanded(false);
            },
          },
        );
      }}
    >
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Milestone name"
        className="rounded border bg-background px-2 py-1 text-sm"
      />
      <textarea
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description (optional)"
        rows={2}
        className="rounded border bg-background p-2 text-sm"
      />
      <div className="flex items-center gap-2">
        <button
          type="submit"
          disabled={!name.trim() || create.isPending}
          className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
        >
          {create.isPending ? "Creating…" : "Create"}
        </button>
        <button
          type="button"
          onClick={() => {
            setExpanded(false);
            setName("");
            setDescription("");
          }}
          className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10"
        >
          Cancel
        </button>
        {create.error && (
          <span className="text-xs text-rose-500">{create.error.message}</span>
        )}
      </div>
    </form>
  );
}

function ClosedIssueRow({ slug, issue }: { slug: string; issue: IssueFile }) {
  const reopen = useReopenIssue(slug);
  const id = issue.display_id;
  const canAct = id != null;
  return (
    <li className="px-3 py-2 text-sm">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="text-muted-foreground tabular-nums">
          {id != null ? `#${id}` : "—"}
        </span>
        <span className="font-medium opacity-70 line-through">{issue.title}</span>
        <span className="ml-auto">
          <button
            type="button"
            disabled={!canAct || reopen.isPending}
            onClick={() => (id != null ? reopen.mutate(id) : undefined)}
            className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
          >
            {reopen.isPending ? "Reopening…" : "Reopen"}
          </button>
        </span>
      </div>
      {reopen.error && (
        <p className="mt-1 text-xs text-rose-500">{reopen.error.message}</p>
      )}
    </li>
  );
}

function LockRow({ slug, lock }: { slug: string; lock: LockEntry }) {
  const release = useReleaseLock(slug);
  const steal = useStealLock(slug);
  const error = release.error?.message ?? steal.error?.message;

  return (
    <li className="flex flex-wrap items-baseline justify-between gap-2 px-3 py-2 text-sm">
      <span>
        #{lock.issue_id} held by <span className="font-medium">{lock.agent_id}</span>
        {lock.branch && <span className="text-muted-foreground"> · {lock.branch}</span>}
      </span>
      <span className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground tabular-nums">
          claimed {lock.claimed_at}
        </span>
        <button
          type="button"
          disabled={release.isPending}
          onClick={() => release.mutate(lock.issue_id)}
          className="rounded border px-2 py-0.5 text-xs hover:bg-accent/10 disabled:opacity-50"
        >
          {release.isPending ? "Releasing…" : "Release"}
        </button>
        <button
          type="button"
          disabled={steal.isPending}
          onClick={() => {
            if (
              window.confirm(
                `Steal lock on #${lock.issue_id} from ${lock.agent_id}? This overrides the other agent.`,
              )
            ) {
              steal.mutate(lock.issue_id);
            }
          }}
          className="rounded border border-amber-500/40 px-2 py-0.5 text-xs text-amber-500 hover:bg-amber-500/10 disabled:opacity-50"
        >
          {steal.isPending ? "Stealing…" : "Steal"}
        </button>
      </span>
      {error && (
        <p className="w-full text-xs text-rose-500">{error}</p>
      )}
    </li>
  );
}

function Counter({ label, value, warn }: { label: string; value: number; warn?: boolean }) {
  return (
    <div className="rounded border bg-card p-2 text-center">
      <div className={`text-xl font-semibold tabular-nums ${warn ? "text-rose-500" : ""}`}>
        {value}
      </div>
      <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
    </div>
  );
}

function FallbackMessage({
  children,
  tone,
}: {
  children: React.ReactNode;
  tone: "info" | "error";
}) {
  return (
    <main className="mx-auto max-w-3xl px-6 py-16">
      <p className={tone === "error" ? "text-rose-500" : "text-muted-foreground"}>{children}</p>
    </main>
  );
}
