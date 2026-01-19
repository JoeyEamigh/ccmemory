import { getDatabase } from "../../db/database.js";
import { log } from "../../utils/log.js";
import { OllamaProvider } from "./ollama.js";
import { OpenRouterProvider } from "./openrouter.js";
import type {
  EmbeddingConfig,
  EmbeddingProvider,
  EmbeddingResult,
  EmbeddingService,
} from "./types.js";
import { DEFAULT_CONFIG as defaultConfig } from "./types.js";

async function registerModel(provider: EmbeddingProvider): Promise<void> {
  const db = await getDatabase();
  const modelId = `${provider.name}:${provider.model}`;

  await db.batch([
    {
      sql: `INSERT INTO embedding_models (id, name, provider, dimensions, is_active)
            VALUES (?, ?, ?, ?, 0)
            ON CONFLICT (id) DO UPDATE SET is_active = 0`,
      args: [modelId, provider.model, provider.name, provider.dimensions],
    },
    {
      sql: `UPDATE embedding_models SET is_active = 0`,
    },
    {
      sql: `UPDATE embedding_models SET is_active = 1 WHERE id = ?`,
      args: [modelId],
    },
  ]);
}

function createService(
  active: EmbeddingProvider,
  providers: Record<"ollama" | "openrouter", EmbeddingProvider>
): EmbeddingService {
  return {
    getProvider(): EmbeddingProvider {
      return active;
    },

    async embed(text: string): Promise<EmbeddingResult> {
      return {
        vector: await active.embed(text),
        model: active.model,
        dimensions: active.dimensions,
        cached: false,
      };
    },

    async embedBatch(texts: string[]): Promise<EmbeddingResult[]> {
      const vectors = await active.embedBatch(texts);
      return vectors.map((v) => ({
        vector: v,
        model: active.model,
        dimensions: active.dimensions,
        cached: false,
      }));
    },

    getActiveModelId(): string {
      return `${active.name}:${active.model}`;
    },

    async switchProvider(provider: "ollama" | "openrouter"): Promise<void> {
      const newProvider = providers[provider];

      if (!(await newProvider.isAvailable())) {
        throw new Error(`Provider ${provider} not available`);
      }

      active = newProvider;
      await registerModel(active);
      log.info("embedding", "Switched provider", { provider: active.name, model: active.model });
    },
  };
}

async function initializeProvider(
  config: EmbeddingConfig
): Promise<{ active: EmbeddingProvider; providers: Record<"ollama" | "openrouter", EmbeddingProvider> } | null> {
  const ollamaProvider = new OllamaProvider(config.ollama);
  const openrouterProvider = new OpenRouterProvider(config.openrouter);

  const providers: Record<"ollama" | "openrouter", EmbeddingProvider> = {
    ollama: ollamaProvider,
    openrouter: openrouterProvider,
  };

  log.debug("embedding", "Checking provider availability", { provider: config.provider });

  let active: EmbeddingProvider = providers[config.provider];

  if (!(await active.isAvailable())) {
    const fallback = config.provider === "ollama" ? "openrouter" : "ollama";
    log.warn("embedding", "Primary provider unavailable, trying fallback", {
      primary: config.provider,
      fallback,
    });

    const fallbackProvider = providers[fallback];
    if (await fallbackProvider.isAvailable()) {
      active = fallbackProvider;
      log.info("embedding", "Using fallback provider", { provider: fallback });
    } else {
      return null;
    }
  }

  return { active, providers };
}

export async function createEmbeddingService(
  config: EmbeddingConfig = defaultConfig
): Promise<EmbeddingService> {
  const result = await initializeProvider(config);

  if (!result) {
    log.error("embedding", "No embedding provider available");
    throw new Error("No embedding provider available");
  }

  const { active, providers } = result;

  await registerModel(active);
  log.info("embedding", "Embedding service initialized", {
    provider: active.name,
    model: active.model,
    dimensions: active.dimensions,
  });

  return createService(active, providers);
}

export async function createEmbeddingServiceOptional(
  config: EmbeddingConfig = defaultConfig
): Promise<EmbeddingService | null> {
  const result = await initializeProvider(config);

  if (!result) {
    log.warn("embedding", "No embedding provider available, running in degraded mode");
    return null;
  }

  const { active, providers } = result;

  await registerModel(active);
  log.info("embedding", "Embedding service initialized", {
    provider: active.name,
    model: active.model,
    dimensions: active.dimensions,
  });

  return createService(active, providers);
}

export { OllamaProvider } from "./ollama.js";
export { OpenRouterProvider } from "./openrouter.js";
export type {
  EmbeddingConfig,
  EmbeddingProvider,
  EmbeddingResult,
  EmbeddingService,
  OllamaConfig,
  OpenRouterConfig,
} from "./types.js";
export { DEFAULT_CONFIG } from "./types.js";
