import { log } from "../../utils/log.js";
import type { EmbeddingProvider, OpenRouterConfig } from "./types.js";

const MODEL_DIMENSIONS: Record<string, number> = {
  "openai/text-embedding-3-small": 1536,
  "openai/text-embedding-3-large": 3072,
  "openai/text-embedding-ada-002": 1536,
};

type OpenRouterEmbeddingData = {
  embedding: number[];
  index: number;
};

type OpenRouterEmbeddingResponse = {
  data: OpenRouterEmbeddingData[];
  model: string;
  usage: {
    prompt_tokens: number;
    total_tokens: number;
  };
};

export class OpenRouterProvider implements EmbeddingProvider {
  readonly name = "openrouter";
  private apiKey: string;
  readonly model: string;
  readonly dimensions: number;

  constructor(config: OpenRouterConfig) {
    this.apiKey = config.apiKey ?? process.env["OPENROUTER_API_KEY"] ?? "";
    this.model = config.model;
    this.dimensions = MODEL_DIMENSIONS[this.model] ?? 1536;
  }

  async isAvailable(): Promise<boolean> {
    if (!this.apiKey) {
      log.warn("embedding", "OpenRouter API key not configured");
      return false;
    }

    try {
      log.debug("embedding", "Checking OpenRouter availability");
      const response = await fetch("https://openrouter.ai/api/v1/models", {
        headers: { Authorization: `Bearer ${this.apiKey}` },
      });
      if (response.ok) {
        log.info("embedding", "OpenRouter provider ready", { model: this.model });
      }
      return response.ok;
    } catch (e) {
      const err = e as Error;
      log.debug("embedding", "OpenRouter check failed", { error: err.message });
      return false;
    }
  }

  async embed(text: string): Promise<number[]> {
    const start = Date.now();
    const response = await fetch("https://openrouter.ai/api/v1/embeddings", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${this.apiKey}`,
        "HTTP-Referer": "https://github.com/user/ccmemory",
        "X-Title": "CCMemory",
      },
      body: JSON.stringify({
        model: this.model,
        input: text,
      }),
    });

    if (!response.ok) {
      log.error("embedding", "OpenRouter embed failed", { status: response.statusText });
      throw new Error(`OpenRouter embed failed: ${response.statusText}`);
    }

    const data = (await response.json()) as OpenRouterEmbeddingResponse;
    log.debug("embedding", "OpenRouter embedded", { length: text.length, ms: Date.now() - start });

    const first = data.data[0];
    if (!first) {
      throw new Error("OpenRouter returned empty embedding response");
    }
    return first.embedding;
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    const start = Date.now();
    log.debug("embedding", "OpenRouter batch embedding", { count: texts.length });

    const response = await fetch("https://openrouter.ai/api/v1/embeddings", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${this.apiKey}`,
        "HTTP-Referer": "https://github.com/user/ccmemory",
        "X-Title": "CCMemory",
      },
      body: JSON.stringify({
        model: this.model,
        input: texts,
      }),
    });

    if (!response.ok) {
      log.error("embedding", "OpenRouter batch embed failed", { status: response.statusText });
      throw new Error(`OpenRouter embed batch failed: ${response.statusText}`);
    }

    const data = (await response.json()) as OpenRouterEmbeddingResponse;
    log.info("embedding", "OpenRouter batch complete", { count: texts.length, ms: Date.now() - start });

    return data.data.map((d) => d.embedding);
  }
}
