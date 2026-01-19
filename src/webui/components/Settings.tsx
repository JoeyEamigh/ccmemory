import { useState, useEffect } from "react";
import {
  Settings as SettingsIcon,
  RefreshCw,
  Brain,
  FolderOpen,
  FileText,
  Clock,
} from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card.js";
import { Button } from "./ui/button.js";
import { Badge } from "./ui/badge.js";
import { cn } from "../lib/utils.js";

type Stats = {
  totals: {
    memories: number;
    projectMemories: number;
    documents: number;
    sessions: number;
  };
  bySector: Record<string, number>;
};

const sectorColors: Record<string, string> = {
  episodic: "bg-sector-episodic",
  semantic: "bg-sector-semantic",
  procedural: "bg-sector-procedural",
  emotional: "bg-sector-emotional",
  reflective: "bg-sector-reflective",
};

const sectorVariants: Record<
  string,
  "episodic" | "semantic" | "procedural" | "emotional" | "reflective"
> = {
  episodic: "episodic",
  semantic: "semantic",
  procedural: "procedural",
  emotional: "emotional",
  reflective: "reflective",
};

export function Settings(): JSX.Element {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(false);

  const loadStats = async (): Promise<void> => {
    setLoading(true);
    try {
      const res = await fetch("/api/stats");
      const data = (await res.json()) as Stats;
      setStats(data);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadStats();
  }, []);

  const totalSectorCount = stats
    ? Object.values(stats.bySector).reduce((a, b) => a + b, 0)
    : 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight flex items-center gap-2">
            <SettingsIcon className="h-6 w-6" />
            Settings
          </h2>
          <p className="text-muted-foreground">
            System statistics and configuration
          </p>
        </div>
        <Button variant="outline" onClick={loadStats} disabled={loading}>
          <RefreshCw
            className={cn("h-4 w-4 mr-2", loading && "animate-spin")}
          />
          Refresh
        </Button>
      </div>

      {stats && (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
          <Card className="relative overflow-hidden">
            <div className="absolute top-0 right-0 w-16 h-16 -mr-4 -mt-4 bg-primary/5 rounded-full" />
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                <Brain className="h-4 w-4" />
                Total Memories
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold tabular-nums">
                {stats.totals.memories.toLocaleString()}
              </div>
            </CardContent>
          </Card>

          <Card className="relative overflow-hidden">
            <div className="absolute top-0 right-0 w-16 h-16 -mr-4 -mt-4 bg-primary/5 rounded-full" />
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                <FolderOpen className="h-4 w-4" />
                Project Memories
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold tabular-nums">
                {stats.totals.projectMemories.toLocaleString()}
              </div>
            </CardContent>
          </Card>

          <Card className="relative overflow-hidden">
            <div className="absolute top-0 right-0 w-16 h-16 -mr-4 -mt-4 bg-primary/5 rounded-full" />
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                <FileText className="h-4 w-4" />
                Documents
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold tabular-nums">
                {stats.totals.documents.toLocaleString()}
              </div>
            </CardContent>
          </Card>

          <Card className="relative overflow-hidden">
            <div className="absolute top-0 right-0 w-16 h-16 -mr-4 -mt-4 bg-primary/5 rounded-full" />
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                <Clock className="h-4 w-4" />
                Sessions
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold tabular-nums">
                {stats.totals.sessions.toLocaleString()}
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {stats && Object.keys(stats.bySector).length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Memories by Sector</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {Object.entries(stats.bySector).map(([sector, count]) => {
              const percentage =
                totalSectorCount > 0 ? (count / totalSectorCount) * 100 : 0;
              return (
                <div key={sector} className="space-y-1">
                  <div className="flex items-center justify-between">
                    <Badge variant={sectorVariants[sector] ?? "default"}>
                      {sector}
                    </Badge>
                    <span className="text-sm font-medium tabular-nums">
                      {count.toLocaleString()}{" "}
                      <span className="text-muted-foreground">
                        ({percentage.toFixed(1)}%)
                      </span>
                    </span>
                  </div>
                  <div className="h-2 rounded-full bg-muted overflow-hidden">
                    <div
                      className={cn(
                        "h-full rounded-full transition-all duration-500",
                        sectorColors[sector] ?? "bg-primary"
                      )}
                      style={{ width: `${percentage}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
