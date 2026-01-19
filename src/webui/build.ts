import { log } from '../utils/log.js';

export type BuildOutput = {
  clientJs: string;
  css: string;
};

let prebuiltAssets: { clientJs: string; css: string } | null = null;

async function loadPrebuiltAssets(): Promise<{ clientJs: string; css: string } | null> {
  if (prebuiltAssets !== null) {
    return prebuiltAssets;
  }

  try {
    const { PREBUILT_ASSETS_AVAILABLE, PREBUILT_CLIENT_JS, PREBUILT_CSS } = await import('./assets.generated.js');

    if (PREBUILT_ASSETS_AVAILABLE) {
      prebuiltAssets = {
        clientJs: PREBUILT_CLIENT_JS,
        css: PREBUILT_CSS,
      };
      return prebuiltAssets;
    }
  } catch {
    log.debug('webui', 'No pre-built assets found, will build at runtime');
  }

  return null;
}

export async function buildAssets(): Promise<BuildOutput> {
  const prebuilt = await loadPrebuiltAssets();
  if (prebuilt) {
    log.info('webui', 'Using pre-built assets');
    return prebuilt;
  }

  throw new Error(
    'Pre-built WebUI assets not found. Run "bun run prebuild:webui" to generate them, or use "bun run dev:serve" for development.',
  );
}
