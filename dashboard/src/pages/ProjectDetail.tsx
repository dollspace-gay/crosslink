// Drill-down view for a single tracked project. Pulls the full
// `ProjectDetail` payload from `/api/v1/dashboard/projects/{*slug}` and
// renders sections for issues, agents, and locks. Intentionally
// minimal — real IA lands with write surfaces (P1.8+).

import { Link, useParams } from "react-router-dom";

import { useProject } from "@/api/client";

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
              <li key={l.issue_id} className="flex items-baseline justify-between px-3 py-2 text-sm">
                <span>
                  #{l.issue_id} held by <span className="font-medium">{l.agent_id}</span>
                  {l.branch && <span className="text-muted-foreground"> · {l.branch}</span>}
                </span>
                <span className="text-xs text-muted-foreground tabular-nums">
                  claimed {l.claimed_at}
                </span>
              </li>
            ))}
          </ul>
        )}
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
              <li key={i.uuid} className="px-3 py-2 text-sm">
                <span className="text-muted-foreground tabular-nums">
                  {i.display_id != null ? `#${i.display_id}` : "—"}
                </span>{" "}
                <span className="font-medium">{i.title}</span>{" "}
                <span className="text-xs text-muted-foreground">[{i.priority}]</span>
                {i.due_at && (
                  <span className="ml-2 text-xs text-muted-foreground">due {i.due_at}</span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
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
