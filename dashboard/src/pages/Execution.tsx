import { useCallback, useEffect, useState } from "react";
import { useOrchestratorStore } from "@/stores/orchestrator";
import { orchestrator as orchestratorApi } from "@/api/client";
import { wsClient } from "@/api/ws";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { DagGraph } from "@/components/DagGraph";
import { GanttChart } from "@/components/GanttChart";
import { Pause, Play, GitBranch, BarChart3, X } from "lucide-react";
import type { OrchestratorPlan, OrchestratorStage, StageStatus, WsServerMessage } from "@/lib/types";

// ---------------------------------------------------------------------------
// View mode toggle
// ---------------------------------------------------------------------------

type ViewMode = "dag" | "gantt";

// ---------------------------------------------------------------------------
// Stage detail sidebar
// ---------------------------------------------------------------------------

interface StageDetailProps {
  plan: OrchestratorPlan;
  stageId: string;
  onClose: () => void;
}

function StageDetail({ plan, stageId, onClose }: StageDetailProps) {
  let found: (OrchestratorStage & { phaseTitle: string }) | null = null;
  for (const phase of plan.phases) {
    const stage = phase.stages.find((s) => s.id === stageId);
    if (stage) {
      found = { ...stage, phaseTitle: phase.title };
      break;
    }
  }

  if (!found) return null;

  return (
    <Card className="w-80 shrink-0">
      <CardHeader className="pb-2 flex flex-row items-start justify-between">
        <div className="min-w-0 flex-1">
          <CardTitle className="text-sm leading-tight">{found.title}</CardTitle>
          <p className="text-xs text-muted-foreground mt-0.5">{found.phaseTitle}</p>
        </div>
        <Button variant="ghost" size="sm" className="h-6 w-6 p-0 shrink-0" onClick={onClose}>
          <X className="h-3.5 w-3.5" />
        </Button>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground">Status</span>
          <Badge variant={badgeVariant(found.status)}>{found.status ?? "pending"}</Badge>
        </div>

        {found.description && (
          <div>
            <span className="text-muted-foreground block mb-0.5">Description</span>
            <p className="text-xs">{found.description}</p>
          </div>
        )}

        <div className="grid grid-cols-2 gap-2 text-xs">
          <div>
            <span className="text-muted-foreground">Complexity</span>
            <p className="font-medium">{found.complexity_hours}h</p>
          </div>
          <div>
            <span className="text-muted-foreground">Agents</span>
            <p className="font-medium">{found.agent_count}</p>
          </div>
        </div>

        {found.agent_id && (
          <div>
            <span className="text-muted-foreground text-xs block mb-0.5">Assigned Agent</span>
            <p className="text-xs font-mono break-all">{found.agent_id}</p>
          </div>
        )}

        {found.depends_on.length > 0 && (
          <div>
            <span className="text-muted-foreground text-xs block mb-0.5">Dependencies</span>
            <ul className="text-xs space-y-0.5">
              {found.depends_on.map((dep) => (
                <li key={dep} className="font-mono text-muted-foreground">{dep}</li>
              ))}
            </ul>
          </div>
        )}

        {found.tasks.length > 0 && (
          <div>
            <span className="text-muted-foreground text-xs block mb-0.5">
              Tasks ({found.tasks.length})
            </span>
            <ul className="text-xs space-y-1">
              {found.tasks.map((task) => (
                <li key={task.id} className="rounded bg-muted/30 px-2 py-1">
                  <p className="font-medium">{task.title}</p>
                  <p className="text-muted-foreground">{task.complexity_hours}h</p>
                </li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function badgeVariant(status: StageStatus | undefined): "success" | "info" | "warning" | "destructive" | "secondary" {
  switch (status) {
    case "done":    return "success";
    case "running": return "info";
    case "blocked": return "warning";
    case "failed":  return "destructive";
    default:        return "secondary";
  }
}

function executionBadgeVariant(status: string): "success" | "info" | "warning" | "destructive" | "secondary" {
  switch (status) {
    case "running": return "info";
    case "paused":  return "warning";
    case "done":    return "success";
    case "failed":  return "destructive";
    default:        return "secondary";
  }
}

/** Compute summary counts from a plan. */
function summarize(plan: OrchestratorPlan) {
  let done = 0;
  let running = 0;
  let failed = 0;
  let total = 0;
  for (const phase of plan.phases) {
    for (const stage of phase.stages) {
      total++;
      if (stage.status === "done") done++;
      else if (stage.status === "running") running++;
      else if (stage.status === "failed") failed++;
    }
  }
  return { done, running, failed, total };
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export function Execution() {
  const { plan, executionStatus, progressPct, fetchPlan, fetchStatus, applyProgress } =
    useOrchestratorStore();

  const [viewMode, setViewMode] = useState<ViewMode>("dag");
  const [selectedStage, setSelectedStage] = useState<string | null>(null);

  // Initial data load
  useEffect(() => {
    void fetchPlan();
    void fetchStatus();
  }, [fetchPlan, fetchStatus]);

  // WebSocket listener for real-time execution progress
  useEffect(() => {
    const off = wsClient.on((msg: WsServerMessage) => {
      if (msg.type === "execution_progress") {
        applyProgress(msg.phase_id, msg.stage_id, msg.status);
        void fetchStatus();
      }
    });
    return () => { off(); };
  }, [applyProgress, fetchStatus]);

  // Fallback polling (if WS is disconnected)
  useEffect(() => {
    if (executionStatus !== "running") return;
    const id = setInterval(() => {
      void fetchStatus();
      void fetchPlan();
    }, 10_000);
    return () => clearInterval(id);
  }, [executionStatus, fetchStatus, fetchPlan]);

  const handlePause = async () => {
    await orchestratorApi.pause().catch(() => {});
    void fetchStatus();
  };

  const handleResume = async () => {
    await orchestratorApi.execute().catch(() => {});
    void fetchStatus();
  };

  const handleStageClick = useCallback((stageId: string) => {
    setSelectedStage((prev) => (prev === stageId ? null : stageId));
  }, []);

  const stats = plan ? summarize(plan) : null;

  return (
    <div className="p-6 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h1 className="text-2xl font-bold">Execution</h1>
        <div className="flex items-center gap-2">
          <Badge variant={executionBadgeVariant(executionStatus)}>{executionStatus}</Badge>
          {executionStatus === "running" && (
            <Button size="sm" variant="outline" onClick={handlePause}>
              <Pause className="h-4 w-4 mr-1" /> Pause
            </Button>
          )}
          {executionStatus === "paused" && (
            <Button size="sm" onClick={handleResume}>
              <Play className="h-4 w-4 mr-1" /> Resume
            </Button>
          )}
        </div>
      </div>

      {/* Progress + summary stats */}
      {executionStatus !== "idle" && stats && (
        <Card>
          <CardContent className="pt-4 space-y-3">
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Overall Progress</span>
              <span className="font-medium">{progressPct}%</span>
            </div>
            <div className="h-2 w-full rounded-full bg-secondary overflow-hidden">
              <div
                className="h-full rounded-full bg-blue-500 transition-all duration-500"
                style={{ width: `${progressPct}%` }}
              />
            </div>
            <div className="flex gap-4 text-xs text-muted-foreground">
              <span>{stats.done}/{stats.total} done</span>
              {stats.running > 0 && (
                <span className="text-blue-400">{stats.running} running</span>
              )}
              {stats.failed > 0 && (
                <span className="text-red-400">{stats.failed} failed</span>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {plan ? (
        <>
          {/* View mode toggle */}
          <div className="flex items-center gap-1 rounded-lg border border-border p-1 w-fit">
            <Button
              size="sm"
              variant={viewMode === "dag" ? "default" : "ghost"}
              className="h-7 px-3 text-xs"
              onClick={() => setViewMode("dag")}
            >
              <GitBranch className="h-3.5 w-3.5 mr-1" /> DAG
            </Button>
            <Button
              size="sm"
              variant={viewMode === "gantt" ? "default" : "ghost"}
              className="h-7 px-3 text-xs"
              onClick={() => setViewMode("gantt")}
            >
              <BarChart3 className="h-3.5 w-3.5 mr-1" /> Gantt
            </Button>
          </div>

          {/* Visualization + optional detail panel */}
          <div className="flex gap-4">
            <div className="flex-1 min-w-0">
              {viewMode === "dag" ? (
                <DagGraph plan={plan} onStageClick={handleStageClick} />
              ) : (
                <GanttChart plan={plan} onStageClick={handleStageClick} />
              )}
            </div>

            {selectedStage && (
              <StageDetail
                plan={plan}
                stageId={selectedStage}
                onClose={() => setSelectedStage(null)}
              />
            )}
          </div>
        </>
      ) : (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground text-sm">
            No execution plan. Go to Orchestrator to import and decompose a design document.
          </CardContent>
        </Card>
      )}
    </div>
  );
}
