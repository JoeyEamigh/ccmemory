import { Brain, Clock, FolderGit2, Moon, Search, Settings, Sun, Users } from 'lucide-react';
import type { ReactNode } from 'react';
import { cn } from '../lib/utils.js';
import { Button } from './ui/button.js';

type LayoutProps = {
  children: ReactNode;
  currentPath: string;
  onNavigate: (path: string) => void;
  wsConnected: boolean;
};

const navItems = [
  { path: '/projects', label: 'Projects', icon: FolderGit2 },
  { path: '/', label: 'Search', icon: Search },
  { path: '/agents', label: 'Agents', icon: Users },
  { path: '/timeline', label: 'Timeline', icon: Clock },
  { path: '/settings', label: 'Settings', icon: Settings },
];

export function Layout({ children, currentPath, onNavigate, wsConnected }: LayoutProps): React.JSX.Element {
  const toggleTheme = (): void => {
    if (typeof document !== 'undefined') {
      const isDark = document.documentElement.classList.toggle('dark');
      localStorage.setItem('ccmemory-theme', isDark ? 'dark' : 'light');
    }
  };

  return (
    <div className="flex min-h-screen flex-col">
      <header className="sticky top-0 z-50 w-full border-b bg-background/95 backdrop-blur-sm supports-backdrop-filter:bg-background/60">
        <div className="container flex h-14 items-center">
          <div className="mr-6 flex items-center gap-2">
            <Brain className="h-6 w-6 text-primary" />
            <span className="text-lg font-semibold">CCMemory</span>
          </div>

          <nav className="flex items-center gap-1">
            {navItems.map(item => {
              const Icon = item.icon;
              const isActive = currentPath === item.path || (item.path === '/' && currentPath.startsWith('/search'));
              return (
                <Button
                  key={item.path}
                  variant={isActive ? 'secondary' : 'ghost'}
                  size="sm"
                  onClick={() => onNavigate(item.path)}
                  className={cn('gap-2', isActive && 'bg-secondary')}>
                  <Icon className="h-4 w-4" />
                  <span className="hidden sm:inline">{item.label}</span>
                </Button>
              );
            })}
          </nav>

          <div className="ml-auto flex items-center gap-2">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className={cn('h-2 w-2 rounded-full', wsConnected ? 'bg-green-500' : 'bg-muted')} />
              <span className="hidden sm:inline">{wsConnected ? 'Connected' : 'Connecting...'}</span>
            </div>

            <Button variant="ghost" size="icon" onClick={toggleTheme}>
              <Sun className="h-4 w-4 scale-100 rotate-0 transition-all dark:scale-0 dark:-rotate-90" />
              <Moon className="absolute h-4 w-4 scale-0 rotate-90 transition-all dark:scale-100 dark:rotate-0" />
              <span className="sr-only">Toggle theme</span>
            </Button>
          </div>
        </div>
      </header>

      <main className="container flex-1 py-6">{children}</main>

      <footer className="border-t py-4">
        <div className="container text-center text-sm text-muted-foreground">CCMemory - Claude Code Memory Plugin</div>
      </footer>
    </div>
  );
}
