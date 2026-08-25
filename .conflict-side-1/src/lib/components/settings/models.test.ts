import { describe, expect, it } from 'vitest';
import {
  buildRecommended,
  CATALOG,
  catalogFor,
  modelDisplayLabel,
  providerSections,
  recommendedModels,
  type CatalogEntry,
} from './models';

describe('buildRecommended', () => {
  it('gives every task/provider pair both tiers', () => {
    for (const section of providerSections) {
      for (const task of section.tasks) {
        const tiers = recommendedModels[task][section.id];
        expect(tiers, `${section.id} has no ${task} entry`).toBeDefined();
        expect(tiers?.premium, `${section.id} ${task} premium`).toBeTruthy();
        expect(tiers?.standard, `${section.id} ${task} standard`).toBeTruthy();
      }
    }
  });

  it('never lets catalog order silently pick a tier winner', () => {
    const duplicated: CatalogEntry[] = [
      ...CATALOG,
      {
        provider: 'groq',
        id: 'some-other-model',
        label: 'Some Other Model',
        tasks: ['transcription'],
        tags: ['accurate'],
        tier: 'premium',
      },
    ];
    expect(() => buildRecommended(duplicated)).toThrow(/Two premium models for groq transcription/);
  });

  it('leaves local models out of preset matching', () => {
    expect(Object.keys(recommendedModels.transcription)).not.toContain('local');
    expect(CATALOG.some((entry) => entry.provider === 'local' && entry.tier)).toBe(false);
  });
});

describe('catalogFor', () => {
  it('only returns models that declare the task', () => {
    expect(catalogFor('cleanup').every((entry) => entry.tasks.includes('cleanup'))).toBe(true);
    // AssemblyAI is transcription-only, so it must never appear under cleanup.
    expect(catalogFor('cleanup', 'assemblyai')).toEqual([]);
  });

  it('lists a dual-task model under both tasks', () => {
    const ids = (task: 'transcription' | 'cleanup') =>
      catalogFor(task, 'google').map((entry) => entry.id);
    expect(ids('transcription')).toContain('gemini-3.7-flash');
    expect(ids('cleanup')).toContain('gemini-3.7-flash');
  });
});

describe('modelDisplayLabel', () => {
  // Every id the old hand-written switch knew, with the name it produced. If
  // the catalog loses one of these, local rows silently regress to raw ids.
  const LEGACY_LABELS: Record<string, string> = {
    'groq/openai/gpt-oss-20b': 'GPT OSS 20B',
    'groq/qwen/qwen3.6-27b': 'Qwen3.6 27B',
    'assemblyai/universal-3-5-pro': 'Universal 3.5 Pro',
    'assemblyai/universal-2': 'Universal-2',
    'local/gemma-4-e2b': 'Gemma 4 E2B',
    'local/gemma-4-e4b': 'Gemma 4 E4B',
    'local/parakeet-v3': 'Parakeet V3',
    'local/parakeet-v2': 'Parakeet V2',
    'local/moonshine-base': 'Moonshine Base',
    'local/moonshine-tiny': 'Moonshine Tiny',
    'local/moonshine-small': 'Moonshine Small',
    'local/moonshine-medium': 'Moonshine Medium',
    'local/sense-voice': 'SenseVoice',
    'local/gigaam-v3': 'GigaAM v3',
    'local/canary-180m-flash': 'Canary 180M Flash',
    'local/canary-1b-v2': 'Canary 1B v2',
    'local/cohere': 'Cohere',
    'local/qwen2.5-0.5b-instruct': 'Qwen 2.5 0.5B Instruct',
    'local/qwen2.5-1.5b-instruct': 'Qwen 2.5 1.5B Instruct',
    'local/qwen2.5-3b-instruct': 'Qwen 2.5 3B Instruct',
    'local/qwen2.5-7b-instruct': 'Qwen 2.5 7B Instruct',
    'local/phi-3-mini-4k-instruct': 'Phi-3 Mini 4K Instruct',
    'local/smollm2-360m-instruct': 'SmolLM2 360M Instruct',
    'local/smollm2-1.7b-instruct': 'SmolLM2 1.7B Instruct',
    'local/granite-3.3-2b-instruct': 'Granite 3.3 2B Instruct',
    'local/granite-3.3-8b-instruct': 'Granite 3.3 8B Instruct',
  };

  it('matches every label the old switch produced', () => {
    for (const [key, expected] of Object.entries(LEGACY_LABELS)) {
      const slash = key.indexOf('/');
      const provider = key.slice(0, slash) as CatalogEntry['provider'];
      expect(modelDisplayLabel(provider, key.slice(slash + 1)), key).toBe(expected);
    }
  });

  it('falls back to the raw id for models it has never heard of', () => {
    expect(modelDisplayLabel('groq', 'distil-whisper-large-v3-en')).toBe(
      'distil-whisper-large-v3-en',
    );
  });
});
