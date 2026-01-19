import { log } from "../../utils/log.js";
import type { EmbeddingProvider, OllamaConfig } from "./types.js";

type OllamaModel = {
  name: string;
  model: string;
  modified_at: string;
  size: number;
};

type OllamaTagsResponse = {
  models: OllamaModel[];
};

type OllamaEmbeddingResponse = {
  embedding: number[];
};

export class OllamaProvider implements EmbeddingProvider {
  readonly name = "ollama";
  private baseUrl: string;
  readonly model: string;
  private _dimensions: number | null = null;

  constructor(config: OllamaConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, "");
    this.model = config.model;
  }

  get dimensions(): number {
    if (this._dimensions === null) {
      throw new Error("Dimensions not yet detected. Call isAvailable() first.");
    }
    return this._dimensions;
  }

  async isAvailable(): Promise<boolean> {
    try {
      log.debug("embedding", "Checking Ollama availability", { url: this.baseUrl });

      const response = await fetch(`${this.baseUrl}/api/tags`);
      if (!response.ok) {
        log.debug("embedding", "Ollama not responding", { status: response.status });
        return false;
      }

      const data = (await response.json()) as OllamaTagsResponse;
      const models = data.models ?? [];

      const hasModel = models.some(
        (m) => m.name === this.model || m.name.startsWith(`${this.model}:`)
      );

      if (!hasModel) {
        log.warn("embedding", "Model not found in Ollama", {
          model: this.model,
          available: models.map((m) => m.name),
        });
        return false;
      }

      const testVec = await this.embedRaw("dimension test");
      this._dimensions = testVec.length;

      log.info("embedding", "Ollama provider ready", {
        model: this.model,
        dimensions: this._dimensions,
      });
      return true;
    } catch (e) {
      const err = e as Error;
      log.debug("embedding", "Ollama check failed", { error: err.message });
      return false;
    }
  }

  private async embedRaw(text: string): Promise<number[]> {
    const response = await fetch(`${this.baseUrl}/api/embeddings`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: this.model,
        prompt: text,
      }),
    });

    if (!response.ok) {
      throw new Error(`Ollama embed failed: ${response.statusText}`);
    }

    const data = (await response.json()) as OllamaEmbeddingResponse;
    return data.embedding;
  }

  async embed(text: string): Promise<number[]> {
    const start = Date.now();
    const embedding = await this.embedRaw(text);
    log.debug("embedding", "Embedded text", { length: text.length, ms: Date.now() - start });
    return embedding;
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    const start = Date.now();
    log.debug("embedding", "Batch embedding", { count: texts.length });
    const results = await Promise.all(texts.map((t) => this.embed(t)));
    log.info("embedding", "Batch complete", { count: texts.length, ms: Date.now() - start });
    return results;
  }
}
