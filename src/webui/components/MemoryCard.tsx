import { Clock, Link2, Zap } from 'lucide-react';
import type { MemorySector } from '../../services/memory/types.js';
import type { SearchResult } from '../../services/search/hybrid.js';
import { cn } from '../lib/utils.js';
import { RelativeTime } from './RelativeTime.js';
import { Badge } from './ui/badge.js';
import { Card, CardContent, CardFooter, CardHeader } from './ui/card.js';

type MemoryCardProps = {
  result: SearchResult;
  onClick: () => void;
};

const sectorVariant: Record<MemorySector, 'episodic' | 'semantic' | 'procedural' | 'emotional' | 'reflective'> = {
  episodic: 'episodic',
  semantic: 'semantic',
  procedural: 'procedural',
  emotional: 'emotional',
  reflective: 'reflective',
};

const sectorBorderColor: Record<MemorySector, string> = {
  episodic: 'border-l-sector-episodic',
  semantic: 'border-l-sector-semantic',
  procedural: 'border-l-sector-procedural',
  emotional: 'border-l-sector-emotional',
  reflective: 'border-l-sector-reflective',
};

export function MemoryCard({ result, onClick }: MemoryCardProps): React.JSX.Element {
  const { memory, score, sourceSession, isSuperseded, supersededBy, relatedMemoryCount } = result;

  return (
    <Card
      className={cn(
        'cursor-pointer border-l-4 transition-all duration-200 hover:bg-accent/30 hover:shadow-md',
        sectorBorderColor[memory.sector],
        isSuperseded && 'opacity-50 grayscale-30',
      )}
      onClick={onClick}>
      <CardHeader className="pb-3">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={sectorVariant[memory.sector]}>{memory.sector}</Badge>

          <div className="flex items-center gap-1 text-xs text-muted-foreground">
            <Zap className="h-3 w-3" />
            <span>{(score * 100).toFixed(0)}%</span>
          </div>

          <div className="flex items-center gap-1" title={`Salience: ${(memory.salience * 100).toFixed(0)}%`}>
            <div className="h-1.5 w-12 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary/60 transition-all"
                style={{ width: `${memory.salience * 100}%` }}
              />
            </div>
          </div>

          {isSuperseded && (
            <Badge variant="destructive" title={`Superseded by ${supersededBy?.id}`} className="px-1.5 text-[10px]">
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
            <RelativeTime timestamp={sourceSession.startedAt} className="ml-auto text-xs text-muted-foreground" />
          )}
        </div>
      </CardHeader>

      <CardContent className="pb-3">
        <p className="line-clamp-3 text-sm leading-relaxed">{memory.summary ?? memory.content}</p>
      </CardContent>

      <CardFooter className="pt-0 text-xs text-muted-foreground">
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          <RelativeTime timestamp={memory.createdAt} />
        </div>
        {memory.tags && memory.tags.length > 0 && (
          <div className="ml-auto flex flex-wrap gap-1">
            {memory.tags.slice(0, 3).map(tag => (
              <span key={tag} className="rounded bg-muted px-1.5 py-0.5 text-[10px]">
                {tag}
              </span>
            ))}
            {memory.tags.length > 3 && <span className="text-[10px]">+{memory.tags.length - 3}</span>}
          </div>
        )}
      </CardFooter>
    </Card>
  );
}
