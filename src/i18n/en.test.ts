import { describe, expect, it } from 'vitest';
import { en } from './en';
import type { SkipReason } from '../types/generated';

const ALL_SKIP_REASONS: readonly SkipReason[] = [
  'NoMatchingRule',
  'RuleDisabled',
  'ProtectedLocation',
  'ExcludedDirectory',
  'ProjectRoot',
  'SortIgnore',
  'HiddenAttribute',
  'SystemAttribute',
  'ReparsePoint',
  'CloudPlaceholder',
  'CloudManagedCrossVolume',
  'TemporaryExtension',
  'SpecialFile',
  'OfficeLockFile',
  'FileInUse',
  'UnstableFile',
  'TooRecent',
  'AlreadyInDestination',
  'DestinationInvalid',
  'DestinationUnavailable',
  'PathTooLong',
  'InvalidName',
  'NonUnicodePath',
  'ReadOnlyCrossVolume',
  'InsufficientSpace',
  'FilesystemLimit',
  'SourceVanished',
  'SourceChanged',
  'PermissionDenied',
  'RestoredByUndo',
  'DepthLimit',
  'ScanLimit',
];

function collectStrings(node: unknown, path = 'en'): Array<{ path: string; text: string }> {
  if (typeof node === 'string') {
    return [{ path, text: node }];
  }
  if (node !== null && typeof node === 'object') {
    return Object.entries(node).flatMap(([key, val]) => collectStrings(val, `${path}.${key}`));
  }
  return [];
}

describe('i18n catalog quality and coverage', () => {
  it('provides a non-empty English message for every SkipReason variant', () => {
    expect(Object.keys(en.skipReasons)).toHaveLength(ALL_SKIP_REASONS.length);
    for (const reason of ALL_SKIP_REASONS) {
      expect(en.skipReasons[reason].trim().length).toBeGreaterThan(0);
    }
  });

  it('contains zero em-dashes, en-dashes, or emoji characters across all UI copy', () => {
    const entries = collectStrings(en);
    const emojiPattern = /\p{Extended_Pictographic}/u;
    for (const { path, text } of entries) {
      expect(text, `em-dash found at ${path}`).not.toContain('\u2014');
      expect(text, `en-dash found at ${path}`).not.toContain('\u2013');
      expect(emojiPattern.test(text), `emoji found at ${path}`).toBe(false);
    }
  });
});
