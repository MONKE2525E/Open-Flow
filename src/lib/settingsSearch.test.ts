import { describe, expect, it } from 'vitest';
import { searchSettings } from './settingsSearch.svelte';

const visibleSections = ['general', 'keys', 'models', 'privacy', 'advanced', 'about'] as const;

describe('settings search', () => {
  it('finds setting content instead of only section names', () => {
    expect(searchSettings('hotkey', visibleSections)[0]).toMatchObject({
      section: 'general',
      target: 'general-hotkey',
    });
  });

  it('finds a specific model and routes it to the relevant model task', () => {
    expect(searchSettings('gpt 4o mini transcribe', visibleSections)[0]).toMatchObject({
      label: 'GPT-4o mini Transcribe',
      section: 'models',
      target: 'models-transcription',
    });
  });

  it('routes color searches to the accent picker', () => {
    expect(searchSettings('orange', visibleSections)[0]).toMatchObject({
      section: 'general',
      target: 'general-accent',
    });
  });

  it('does not return results from hidden settings sections', () => {
    expect(searchSettings('notification test', visibleSections)).toEqual([]);
  });
});
