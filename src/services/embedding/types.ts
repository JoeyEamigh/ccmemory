export type OllamaConfig = {
  baseUrl: string;
  model: string;
};

export type OpenRouterConfig = {
  apiKey?: string;
  model: string;
};

export type EmbeddingConfig = {
  provider: 'ollama' | 'openrouter';
  ollama: OllamaConfig;
  openrouter: OpenRouterConfig;
};

export type EmbeddingProvider = {
  readonly name: string;
  readonly model: string;
  readonly dimensions: number;

  embed(text: string): Promise<number[]>;
  embedBatch(texts: string[]): Promise<number[][]>;
  isAvailable(): Promise<boolean>;
};

export type EmbeddingResult = {
  vector: number[];
  model: string;
  dimensions: number;
  cached: boolean;
};

export type EmbeddingService = {
  getProvider(): EmbeddingProvider;
  embed(text: string): Promise<EmbeddingResult>;
  embedBatch(texts: string[]): Promise<EmbeddingResult[]>;
  getActiveModelId(): string;
  switchProvider(provider: 'ollama' | 'openrouter'): Promise<void>;
};

export const DEFAULT_CONFIG: EmbeddingConfig = {
  provider: 'ollama',
  ollama: {
    baseUrl: 'http://localhost:11434',
    model: 'qwen3-embedding',
  },
  openrouter: {
    model: 'openai/text-embedding-3-small',
  },
};
