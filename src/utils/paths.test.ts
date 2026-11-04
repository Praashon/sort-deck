import { describe, expect, it } from 'vitest';
import { messages } from '../i18n';
import { formatBytes } from './format';
import { resolveErrorMessage, sanitizeFilenameForDisplay, truncateMiddle } from './paths';

describe('filename sanitization and formatting utilities', () => {
  it('escapes bidi override and control characters visibly (§10.4)', () => {
    const spoofed = 'invoice\u202Efdp.exe';
    expect(sanitizeFilenameForDisplay(spoofed)).toBe('invoice[U+202E]fdp.exe');

    const withControls = 'report\u0000\u0007\u2066name\u2069.pdf';
    expect(sanitizeFilenameForDisplay(withControls)).toBe(
      'report[U+0000][U+0007][U+2066]name[U+2069].pdf',
    );

    const unicodeClean = 'résumé-日本語-2026.pdf';
    expect(sanitizeFilenameForDisplay(unicodeClean)).toBe(unicodeClean);
  });

  it('middle-truncates long paths cleanly without overflowing', () => {
    const longPath =
      'C:\\Users\\Example\\Documents\\Archive\\2026\\QuarterFour\\Reports\\annual-summary-final.pdf';
    const short = truncateMiddle(longPath, 42);
    expect(short.length).toBeLessThanOrEqual(42);
    expect(short).toContain('…');
    expect(short.endsWith('annual-summary-final.pdf')).toBe(true);
  });

  it('maps typed AppErrorPayloads through i18n codes', () => {
    expect(
      resolveErrorMessage({
        code: 'HASH_MISMATCH',
        messageKey: 'errors.HASH_MISMATCH',
        details: null,
      }),
    ).toBe(messages.errors.HASH_MISMATCH);

    expect(
      resolveErrorMessage(
        JSON.stringify({
          code: 'DISK_FULL',
          messageKey: 'errors.DISK_FULL',
          details: 'D:\\',
        }),
      ),
    ).toBe(`${messages.errors.DISK_FULL} (D:\\)`);
  });

  it('formats byte counts cleanly with Intl', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
  });
});
