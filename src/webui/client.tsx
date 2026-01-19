import { hydrateRoot } from 'react-dom/client';
import { App } from './components/App.js';

declare global {
  interface Window {
    __INITIAL_DATA__: unknown;
  }
}

function initializeTheme(): void {
  const stored = localStorage.getItem('ccmemory-theme');
  const systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;

  const shouldBeDark = stored === 'dark' || (!stored && systemPrefersDark);

  if (shouldBeDark) {
    document.documentElement.classList.add('dark');
  } else {
    document.documentElement.classList.remove('dark');
  }

  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', e => {
    if (!localStorage.getItem('ccmemory-theme')) {
      if (e.matches) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
    }
  });
}

initializeTheme();

const initialData = window.__INITIAL_DATA__;
const rootElement = document.getElementById('root');

if (rootElement) {
  hydrateRoot(rootElement, <App url={window.location.pathname} initialData={initialData} />);
}
