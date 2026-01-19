import { FolderGit2, RefreshCw, Search } from 'lucide-react';
import { useEffect, useState } from 'react';
import { cn } from '../lib/utils.js';
import { ProjectCard } from './ProjectCard.js';
import { Button } from './ui/button.js';
import { Input } from './ui/input.js';

type Project = {
  id: string;
  path: string;
  name?: string;
  memory_count: number;
  session_count: number;
  last_activity?: number;
  created_at: number;
};

type ProjectsProps = {
  initialProjects: Project[];
  onSelectProject: (projectId: string) => void;
  onNavigate: (path: string) => void;
  wsConnected: boolean;
};

export function Projects({ initialProjects, onSelectProject, onNavigate, wsConnected }: ProjectsProps): React.JSX.Element {
  const [projects, setProjects] = useState(initialProjects);
  const [filter, setFilter] = useState('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setProjects(initialProjects);
  }, [initialProjects]);

  const refresh = async (): Promise<void> => {
    setLoading(true);
    try {
      const res = await fetch('/api/projects');
      const data = (await res.json()) as { projects: Project[] };
      setProjects(data.projects);
    } finally {
      setLoading(false);
    }
  };

  const filtered = projects.filter(p => {
    const search = filter.toLowerCase();
    return p.path.toLowerCase().includes(search) || (p.name?.toLowerCase().includes(search) ?? false);
  });

  const totalMemories = projects.reduce((sum, p) => sum + p.memory_count, 0);
  const totalSessions = projects.reduce((sum, p) => sum + p.session_count, 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="flex items-center gap-2 text-2xl font-semibold tracking-tight">
            <FolderGit2 className="h-6 w-6" />
            Projects
          </h2>
          <p className="text-muted-foreground">
            {projects.length} projects with {totalMemories.toLocaleString()} memories across{' '}
            {totalSessions.toLocaleString()} sessions
          </p>
        </div>
        <div className="flex items-center gap-2">
          {wsConnected ? (
            <span className="flex items-center gap-1.5 text-xs text-green-600 dark:text-green-400">
              <span className="h-2 w-2 animate-pulse rounded-full bg-green-500" />
              Live
            </span>
          ) : (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className="h-2 w-2 rounded-full bg-muted" />
              Offline
            </span>
          )}
          <Button variant="outline" size="sm" onClick={refresh} disabled={loading}>
            <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
          </Button>
        </div>
      </div>

      <div className="relative">
        <Search className="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          placeholder="Filter projects..."
          value={filter}
          onChange={e => setFilter(e.target.value)}
          className="pl-9"
        />
      </div>

      {filtered.length === 0 ? (
        <div className="py-12 text-center text-muted-foreground">
          {filter ? 'No projects match your filter.' : 'No projects found.'}
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map(project => (
            <ProjectCard
              key={project.id}
              project={project}
              onClick={() => onSelectProject(project.id)}
              onViewMemories={() => onNavigate(`/search?project=${project.id}`)}
              onViewTimeline={() => onNavigate(`/timeline?project=${project.id}`)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
