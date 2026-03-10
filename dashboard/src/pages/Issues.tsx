import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { Plus, CircleDot, CheckCircle2, Tag, Milestone, CheckSquare, Square } from "lucide-react";
import { useIssuesStore } from "@/stores/issues";
import { issues as issuesApi, milestones as milestonesApi } from "@/api/client";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { formatRelativeTime } from "@/lib/utils";
import type { IssuePriority, MilestoneDetail } from "@/lib/types";

const PRIORITY_ORDER: Record<IssuePriority, number> = {
  critical: 0, high: 1, medium: 2, low: 3,
};

function priorityVariant(p: IssuePriority) {
  switch (p) {
    case "critical": return "destructive" as const;
    case "high": return "warning" as const;
    case "medium": return "info" as const;
    default: return "secondary" as const;
  }
}

// ---------------------------------------------------------------------------
// Bulk action bar
// ---------------------------------------------------------------------------

interface BulkBarProps {
  selectedIds: Set<number>;
  onClear: () => void;
  onClose: () => void;
  onLabel: () => void;
  onMilestone: () => void;
  busy: boolean;
}

function BulkBar({ selectedIds, onClear, onClose, onLabel, onMilestone, busy }: BulkBarProps) {
  if (selectedIds.size === 0) return null;
  return (
    <div className="flex items-center gap-3 rounded-md border border-border bg-accent/40 px-4 py-2 text-sm">
      <span className="font-medium">{selectedIds.size} selected</span>
      <div className="flex gap-1 ml-2">
        <Button size="sm" variant="outline" className="h-7 gap-1" onClick={onClose} disabled={busy}>
          <CheckCircle2 className="h-3 w-3" />
          Close all
        </Button>
        <Button size="sm" variant="outline" className="h-7 gap-1" onClick={onLabel} disabled={busy}>
          <Tag className="h-3 w-3" />
          Add label
        </Button>
        <Button size="sm" variant="outline" className="h-7 gap-1" onClick={onMilestone} disabled={busy}>
          <Milestone className="h-3 w-3" />
          Assign milestone
        </Button>
      </div>
      <button
        type="button"
        className="ml-auto text-xs text-muted-foreground hover:text-foreground"
        onClick={onClear}
      >
        Clear
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function Issues() {
  const { issues, loading, fetch } = useIssuesStore();
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<"open" | "closed" | "all">("open");

  // Bulk selection
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [bulkBusy, setBulkBusy] = useState(false);

  // Bulk label dialog
  const [labelDialogOpen, setLabelDialogOpen] = useState(false);
  const [bulkLabel, setBulkLabel] = useState("");

  // Bulk milestone dialog
  const [milestoneDialogOpen, setMilestoneDialogOpen] = useState(false);
  const [milestones, setMilestones] = useState<MilestoneDetail[]>([]);
  const [milestonesLoading, setMilestonesLoading] = useState(false);

  const refetch = () => fetch({ status: statusFilter === "all" ? undefined : statusFilter });

  useEffect(() => {
    void refetch();
    // Clear selection when filter changes
    setSelected(new Set());
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [statusFilter]);

  const filtered = issues
    .filter((i) =>
      search === "" ||
      i.title.toLowerCase().includes(search.toLowerCase()) ||
      String(i.id).includes(search),
    )
    .sort((a, b) => PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority]);

  const toggleSelect = (id: number, e: React.MouseEvent) => {
    e.preventDefault();
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selected.size === filtered.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(filtered.map((i) => i.id)));
    }
  };

  // ── Single-row close ────────────────────────────────────────────────────

  const handleClose = async (id: number, e: React.MouseEvent) => {
    e.preventDefault();
    await issuesApi.close(id);
    void refetch();
  };

  // ── Bulk close ──────────────────────────────────────────────────────────

  const handleBulkClose = async () => {
    setBulkBusy(true);
    try {
      await Promise.all([...selected].map((id) => issuesApi.close(id)));
      setSelected(new Set());
      void refetch();
    } finally {
      setBulkBusy(false);
    }
  };

  // ── Bulk label ──────────────────────────────────────────────────────────

  const openLabelDialog = () => {
    setBulkLabel("");
    setLabelDialogOpen(true);
  };

  const handleBulkLabel = async () => {
    const label = bulkLabel.trim().toLowerCase();
    if (!label) return;
    setBulkBusy(true);
    try {
      await Promise.all([...selected].map((id) => issuesApi.addLabel(id, label)));
      setSelected(new Set());
      setLabelDialogOpen(false);
      void refetch();
    } finally {
      setBulkBusy(false);
    }
  };

  // ── Bulk milestone ──────────────────────────────────────────────────────

  const openMilestoneDialog = async () => {
    setMilestonesLoading(true);
    setMilestoneDialogOpen(true);
    try {
      const data = await milestonesApi.list();
      setMilestones(data.filter((m) => m.status === "open"));
    } finally {
      setMilestonesLoading(false);
    }
  };

  const handleBulkMilestone = async (milestoneId: number) => {
    setBulkBusy(true);
    try {
      await Promise.all(
        [...selected].map((issueId) => milestonesApi.assign(milestoneId, issueId)),
      );
      setSelected(new Set());
      setMilestoneDialogOpen(false);
      void refetch();
    } finally {
      setBulkBusy(false);
    }
  };

  // ── Render ──────────────────────────────────────────────────────────────

  const allSelected = filtered.length > 0 && selected.size === filtered.length;
  const someSelected = selected.size > 0 && !allSelected;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Issues</h1>
        <Button size="sm">
          <Plus className="h-4 w-4 mr-1" /> New Issue
        </Button>
      </div>

      <div className="flex items-center gap-3">
        <Input
          placeholder="Search issues…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="max-w-xs"
        />
        <div className="flex gap-1">
          {(["open", "closed", "all"] as const).map((s) => (
            <Button
              key={s}
              size="sm"
              variant={statusFilter === s ? "secondary" : "ghost"}
              onClick={() => setStatusFilter(s)}
              className="capitalize"
            >
              {s}
            </Button>
          ))}
        </div>
      </div>

      {/* Bulk action bar */}
      <BulkBar
        selectedIds={selected}
        onClear={() => setSelected(new Set())}
        onClose={() => void handleBulkClose()}
        onLabel={openLabelDialog}
        onMilestone={() => void openMilestoneDialog()}
        busy={bulkBusy}
      />

      {loading ? (
        <p className="text-muted-foreground text-sm">Loading…</p>
      ) : filtered.length === 0 ? (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground text-sm">
            No issues found.
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-1">
          {/* Select-all header */}
          <div className="flex items-center gap-2 px-4 py-1">
            <SelectCheckbox
              checked={allSelected}
              indeterminate={someSelected}
              onChange={toggleSelectAll}
              aria-label="Select all"
            />
            <span className="text-xs text-muted-foreground">
              {filtered.length} issue{filtered.length !== 1 ? "s" : ""}
            </span>
          </div>

          {filtered.map((issue) => (
            <div
              key={issue.id}
              className="flex items-center gap-2 rounded-md border border-border bg-card hover:bg-accent/30 transition-colors"
            >
              {/* Checkbox */}
              <div
                className="pl-4 py-3 shrink-0"
                onClick={(e) => toggleSelect(issue.id, e)}
              >
                <SelectCheckbox
                  checked={selected.has(issue.id)}
                  onChange={() => {}}
                  aria-label={`Select issue #${issue.id}`}
                />
              </div>

              {/* Issue row — navigates on click */}
              <Link
                to={`/issues/${issue.id}`}
                className="flex flex-1 items-center gap-3 py-3 pr-4 min-w-0"
              >
                {issue.status === "open" ? (
                  <CircleDot className="h-4 w-4 text-green-400 shrink-0" />
                ) : (
                  <CheckCircle2 className="h-4 w-4 text-muted-foreground shrink-0" />
                )}
                <span className="font-mono text-xs text-muted-foreground w-8 shrink-0">
                  #{issue.id}
                </span>
                <span className="flex-1 text-sm truncate">{issue.title}</span>
                <Badge variant={priorityVariant(issue.priority)} className="shrink-0">
                  {issue.priority}
                </Badge>
                <span className="text-xs text-muted-foreground shrink-0">
                  {formatRelativeTime(issue.updated_at)}
                </span>
                {issue.status === "open" && (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-6 px-2 text-xs shrink-0"
                    onClick={(e) => void handleClose(issue.id, e)}
                  >
                    Close
                  </Button>
                )}
              </Link>
            </div>
          ))}
        </div>
      )}

      {/* ── Bulk label dialog ────────────────────────────────────────────── */}
      <Dialog open={labelDialogOpen} onOpenChange={setLabelDialogOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Add label to {selected.size} issue{selected.size !== 1 ? "s" : ""}</DialogTitle>
          </DialogHeader>
          <div className="py-2">
            <Input
              placeholder="Label name…"
              value={bulkLabel}
              onChange={(e) => setBulkLabel(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleBulkLabel();
              }}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setLabelDialogOpen(false)}>
              Cancel
            </Button>
            <Button
              disabled={!bulkLabel.trim() || bulkBusy}
              onClick={() => void handleBulkLabel()}
            >
              Add label
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ── Bulk milestone dialog ────────────────────────────────────────── */}
      <Dialog open={milestoneDialogOpen} onOpenChange={setMilestoneDialogOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>
              Assign milestone to {selected.size} issue{selected.size !== 1 ? "s" : ""}
            </DialogTitle>
          </DialogHeader>
          <div className="py-2 space-y-1">
            {milestonesLoading ? (
              <p className="text-sm text-muted-foreground">Loading milestones…</p>
            ) : milestones.length === 0 ? (
              <p className="text-sm text-muted-foreground">No open milestones found.</p>
            ) : (
              milestones.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm hover:bg-accent transition-colors text-left"
                  disabled={bulkBusy}
                  onClick={() => void handleBulkMilestone(m.id)}
                >
                  <Milestone className="h-4 w-4 text-muted-foreground shrink-0" />
                  <span className="flex-1">{m.name}</span>
                  <span className="text-xs text-muted-foreground">
                    {m.completed_count}/{m.issue_count}
                  </span>
                </button>
              ))
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setMilestoneDialogOpen(false)}>
              Cancel
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ---------------------------------------------------------------------------
// SelectCheckbox — accessible checkbox using a button + icon
// ---------------------------------------------------------------------------

interface SelectCheckboxProps {
  checked: boolean;
  indeterminate?: boolean;
  onChange: (checked: boolean) => void;
  "aria-label"?: string;
}

function SelectCheckbox({ checked, indeterminate, onChange, "aria-label": ariaLabel }: SelectCheckboxProps) {
  const ref = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (ref.current) {
      // Reflect indeterminate state via aria
      ref.current.setAttribute("aria-checked", indeterminate ? "mixed" : String(checked));
    }
  }, [checked, indeterminate]);

  return (
    <button
      ref={ref}
      type="button"
      role="checkbox"
      aria-label={ariaLabel}
      aria-checked={indeterminate ? "mixed" : checked}
      className="h-4 w-4 shrink-0 text-muted-foreground hover:text-foreground transition-colors"
      onClick={() => onChange(!checked)}
    >
      {checked || indeterminate ? (
        <CheckSquare className="h-4 w-4 text-blue-400" />
      ) : (
        <Square className="h-4 w-4" />
      )}
    </button>
  );
}
