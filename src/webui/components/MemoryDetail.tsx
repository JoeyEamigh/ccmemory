import { Archive, Clock, Eye, File, FileCode, FileText, History, Info, Minus, Plus, Tag, Trash2 } from 'lucide-react';
import type { Memory, MemorySector, MemoryType } from '../../services/memory/types.js';
import { Badge } from './ui/badge.js';
import { Button } from './ui/button.js';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog.js';

const memoryTypeLabels: Record<MemoryType, string> = {
  preference: 'Preference',
  codebase: 'Codebase',
  decision: 'Decision',
  gotcha: 'Gotcha',
  pattern: 'Pattern',
  turn_summary: 'Turn Summary',
  task_completion: 'Task Completion',
};

type MemoryDetailProps = {
  memory: Memory;
  onClose: () => void;
  onReinforce: (id: string) => void;
  onDeemphasize: (id: string) => void;
  onDelete: (id: string, hard: boolean) => void;
  onViewTimeline: (id: string) => void;
};

const sectorVariant: Record<MemorySector, 'episodic' | 'semantic' | 'procedural' | 'emotional' | 'reflective'> = {
  episodic: 'episodic',
  semantic: 'semantic',
  procedural: 'procedural',
  emotional: 'emotional',
  reflective: 'reflective',
};

export function MemoryDetail({
  memory,
  onClose,
  onReinforce,
  onDeemphasize,
  onDelete,
  onViewTimeline,
}: MemoryDetailProps): React.JSX.Element {
  const handleDelete = (hard: boolean): void => {
    const msg = hard ? 'Permanently delete this memory? This cannot be undone.' : 'Archive this memory?';
    if (typeof window !== 'undefined' && window.confirm(msg)) {
      onDelete(memory.id, hard);
      onClose();
    }
  };

  return (
    <Dialog open onOpenChange={open => !open && onClose()}>
      <DialogContent className="max-h-[80vh] max-w-2xl overflow-y-auto">
        <DialogHeader>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={sectorVariant[memory.sector]}>{memory.sector}</Badge>
            {memory.memoryType && (
              <Badge variant="outline">{memoryTypeLabels[memory.memoryType] ?? memory.memoryType}</Badge>
            )}
            <Badge variant="secondary">{memory.tier}</Badge>
            {memory.confidence !== undefined && memory.confidence < 1 && (
              <span className="text-xs text-muted-foreground">
                {(memory.confidence * 100).toFixed(0)}% confident
              </span>
            )}
            {memory.isDeleted && <Badge variant="destructive">DELETED</Badge>}
            {memory.validUntil && <Badge variant="destructive">SUPERSEDED</Badge>}
          </div>
          <DialogTitle className="sr-only">Memory Details</DialogTitle>
          <DialogDescription className="sr-only">View and manage memory details</DialogDescription>
        </DialogHeader>

        {memory.summary && memory.summary !== memory.content && (
          <div className="rounded-lg border bg-muted/50 p-3">
            <div className="mb-1 flex items-center gap-1 text-xs font-medium text-muted-foreground">
              <FileText className="h-3 w-3" />
              Summary
            </div>
            <p className="text-sm">{memory.summary}</p>
          </div>
        )}

        <div className="prose prose-sm dark:prose-invert max-w-none py-4">
          <p className="whitespace-pre-wrap">{memory.content}</p>
        </div>

        {memory.context && (
          <div className="rounded-lg border bg-blue-50/50 p-3 dark:bg-blue-950/20">
            <div className="mb-1 flex items-center gap-1 text-xs font-medium text-blue-600 dark:text-blue-400">
              <Info className="h-3 w-3" />
              Context
            </div>
            <p className="text-sm text-blue-800 dark:text-blue-200">{memory.context}</p>
          </div>
        )}

        {memory.concepts && memory.concepts.length > 0 && (
          <div className="rounded-lg border bg-purple-50/50 p-3 dark:bg-purple-950/20">
            <div className="mb-2 flex items-center gap-1 text-xs font-medium text-purple-600 dark:text-purple-400">
              <FileCode className="h-3 w-3" />
              Concepts
            </div>
            <div className="flex flex-wrap gap-1.5">
              {memory.concepts.map(concept => (
                <span
                  key={concept}
                  className="rounded-full bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary">
                  {concept}
                </span>
              ))}
            </div>
          </div>
        )}

        {memory.tags && memory.tags.length > 0 && (
          <div className="rounded-lg border bg-green-50/50 p-3 dark:bg-green-950/20">
            <div className="mb-2 flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
              <Tag className="h-3 w-3" />
              Tags
            </div>
            <div className="flex flex-wrap gap-1.5">
              {memory.tags.map(tag => (
                <span
                  key={tag}
                  className="rounded-full bg-green-100 px-2.5 py-1 text-xs font-medium text-green-700 dark:bg-green-900 dark:text-green-300">
                  {tag}
                </span>
              ))}
            </div>
          </div>
        )}

        {memory.files && memory.files.length > 0 && (
          <div className="rounded-lg border bg-amber-50/50 p-3 dark:bg-amber-950/20">
            <div className="mb-2 flex items-center gap-1 text-xs font-medium text-amber-600 dark:text-amber-400">
              <File className="h-3 w-3" />
              Related Files
            </div>
            <div className="space-y-1">
              {memory.files.map(file => (
                <div
                  key={file}
                  className="rounded bg-amber-100/50 px-2 py-1 font-mono text-xs text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">
                  {file}
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="space-y-3 border-t pt-4">
          <div className="flex items-center justify-between">
            <span className="text-sm text-muted-foreground">Salience</span>
            <div className="flex items-center gap-2">
              <span className="font-medium">{(memory.salience * 100).toFixed(0)}%</span>
              <Button variant="outline" size="icon" className="h-7 w-7" onClick={() => onReinforce(memory.id)}>
                <Plus className="h-3 w-3" />
              </Button>
              <Button variant="outline" size="icon" className="h-7 w-7" onClick={() => onDeemphasize(memory.id)}>
                <Minus className="h-3 w-3" />
              </Button>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2 text-sm">
            <div className="flex items-center gap-2 text-muted-foreground">
              <Clock className="h-4 w-4" />
              <span>Created: {formatDate(memory.createdAt)}</span>
            </div>
            <div className="flex items-center gap-2 text-muted-foreground">
              <Eye className="h-4 w-4" />
              <span>Accessed: {memory.accessCount} times</span>
            </div>
            <div className="col-span-2 flex items-center gap-2 text-muted-foreground">
              <History className="h-4 w-4" />
              <span>Last accessed: {formatDate(memory.lastAccessed)}</span>
            </div>
          </div>
        </div>

        <DialogFooter className="flex-col gap-2 sm:flex-row">
          <Button variant="outline" onClick={() => onViewTimeline(memory.id)}>
            <History className="mr-2 h-4 w-4" />
            View Timeline
          </Button>
          <div className="ml-auto flex gap-2">
            <Button variant="secondary" onClick={() => handleDelete(false)}>
              <Archive className="mr-2 h-4 w-4" />
              Archive
            </Button>
            <Button variant="destructive" onClick={() => handleDelete(true)}>
              <Trash2 className="mr-2 h-4 w-4" />
              Delete Forever
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatDate(ts: number): string {
  if (!ts || !Number.isFinite(ts)) return '';
  const t = ts < 1e12 ? ts * 1000 : ts;
  const date = new Date(t);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleString();
}
