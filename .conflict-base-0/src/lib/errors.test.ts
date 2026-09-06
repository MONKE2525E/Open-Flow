import { describe, expect, it } from 'vitest';
import { classifyIpcError } from './errors';

describe('storage-full errors', () => {
  it('recognizes the backend marker', () => {
    expect(classifyIpcError('STORAGE_FULL: simulated settings write failure')).toEqual({
      kind: 'storage-full',
      message: 'Change failed because the drive is full. Free up storage, then try again.',
    });
  });

  it('recognizes native full-disk wording', () => {
    expect(classifyIpcError('Failed to write settings: No space left on device').kind).toBe(
      'storage-full',
    );
  });
});
