import { Link2, Clock, Zap } from "lucide-react";
import { Card, CardContent, CardFooter, CardHeader } from "./ui/card.js";
import { Badge } from "./ui/badge.js";
import { cn } from "../lib/utils.js";
import type { SearchResult } from "../../services/search/hybrid.js";
import type { MemorySector } from "../../services/memory/types.js";

type MemoryCardProps = {
  result: SearchResult;
  onClick: () => void;
};

const sectorVariant: Record<
  MemorySector,
  "episodic" | "semantic" | "procedural" | "emotional" | "reflective"
> = {
  episodic: "episodic",
  semantic: "semantic",
  procedural: "procedural",
  emotional: "emotional",
  reflective: "reflective",
};

const sectorBorderColor: Record<MemorySector, string> = {
  episodic: "border-l-sector-episodic",
  semantic: "border-l-sector-semantic",
  procedural: "border-l-sector-procedural",
  emotional: "border-l-sector-emotional",
  reflective: "border-l-sector-reflective",
};

export function MemoryCard({ result, onClick }: MemoryCardProps): JSX.Element {
  const {
    memory,
    score,
    sourceSession,
    isSuperseded,
    supersededBy,
    relatedMemoryCount,
  } = result;

  return (
    <Card
      className={cn(
        "cursor-pointer transition-all duration-200 hover:shadow-md hover:bg-accent/30 border-l-4",
        sectorBorderColor[memory.sector],
        isSuperseded && "opacity-50 grayscale-[30%]"
      )}
      onClick={onClick}
    >
      <CardHeader className="pb-3">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={sectorVariant[memory.sector]}>{memory.sector}</Badge>

          <div className="flex items-center gap-1 text-xs text-muted-foreground">
            <Zap className="h-3 w-3" />
            <span>{(score * 100).toFixed(0)}%</span>
          </div>

          <div
            className="flex items-center gap-1"
            title={`Salience: ${(memory.salience * 100).toFixed(0)}%`}
          >
            <div className="h-1.5 w-12 rounded-full bg-muted overflow-hidden">
              <div
                className="h-full bg-primary/60 rounded-full transition-all"
                style={{ width: `${memory.salience * 100}%` }}
              />
            </div>
          </div>

          {isSuperseded && (
            <Badge
              variant="destructive"
              title={`Superseded by ${supersededBy?.id}`}
              className="text-[10px] px-1.5"
            >
              SUPERSEDED
            </Badge>
          )}

          {relatedMemoryCount > 0 && (
            <Badge variant="secondary" className="flex items-center gap-1 text-[10px]">
              <Link2 className="h-2.5 w-2.5" />
              {relatedMemoryCount}
            </Badge>
          )}

          {sourceSession && (
            <span
              className="text-xs text-muted-foreground ml-auto"
              title={sourceSession.summary ?? ""}
            >
              {formatDate(sourceSession.startedAt)}
            </span>
          )}
        </div>
      </CardHeader>

      <CardContent className="pb-3">
        <p className="text-sm leading-relaxed line-clamp-3">
          {memory.summary ?? memory.content}
        </p>
      </CardContent>

      <CardFooter className="pt-0 text-xs text-muted-foreground">
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          <span>{formatDate(memory.createdAt)}</span>
        </div>
        {memory.tags && memory.tags.length > 0 && (
          <div className="ml-auto flex gap-1 flex-wrap">
            {memory.tags.slice(0, 3).map((tag) => (
              <span key={tag} className="px-1.5 py-0.5 rounded bg-muted text-[10px]">
                {tag}
              </span>
            ))}
            {memory.tags.length > 3 && (
              <span className="text-[10px]">+{memory.tags.length - 3}</span>
            )}
          </div>
        )}
      </CardFooter>
    </Card>
  );
}

function formatDate(ts: number): string {
  const date = new Date(ts < 1e12 ? ts * 1000 : ts);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) {
    const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
    if (diffHours === 0) {
      const diffMins = Math.floor(diffMs / (1000 * 60));
      return diffMins <= 1 ? "just now" : `${diffMins}m ago`;
    }
    return `${diffHours}h ago`;
  }
  if (diffDays === 1) return "yesterday";
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: date.getFullYear() !== now.getFullYear() ? "numeric" : undefined,
  });
}
