import { getErrorMessage } from '../i18n';
import type { AppErrorPayload } from '../types/generated';

/**
 * Escapes Unicode bidirectional override/isolate controls and ASCII/C1 control
 * characters into visible `[U+XXXX]` tokens so filenames cannot spoof extensions
 * or corrupt UI layout (`AGENTS.md` §9.2, §10.4).
 */
export function sanitizeFilenameForDisplay(input: string): string {
  let out = '';
  for (let i = 0; i < input.length; i += 1) {
    const code = input.charCodeAt(i);
    const isAsciiControl = (code >= 0x0000 && code <= 0x001f) || (code >= 0x007f && code <= 0x009f);
    const isBidiControl =
      code === 0x061c ||
      code === 0x200e ||
      code === 0x200f ||
      (code >= 0x202a && code <= 0x202e) ||
      (code >= 0x2066 && code <= 0x2069);

    if (isAsciiControl || isBidiControl) {
      out += `[U+${code.toString(16).toUpperCase().padStart(4, '0')}]`;
    } else {
      out += input.charAt(i);
    }
  }
  return out;
}

/**
 * Middle-truncates long paths while keeping the root/prefix and final filename
 * readable (`AGENTS.md` §10.4).
 */
export function truncateMiddle(input: string, maxLength = 64): string {
  const safe = sanitizeFilenameForDisplay(input);
  if (safe.length <= maxLength || maxLength < 12) {
    return safe;
  }
  const keepEnd = Math.min(maxLength - 8, Math.max(14, Math.floor(maxLength * 0.62)));
  const keepStart = maxLength - keepEnd - 1;
  return `${safe.slice(0, keepStart)}…${safe.slice(safe.length - keepEnd)}`;
}

function isAppErrorPayload(value: unknown): value is AppErrorPayload {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    typeof (value as { code?: unknown }).code === 'string'
  );
}

/**
 * Resolves a Tauri/backend error into a localized string using `AppErrorPayload.code`
 * rather than parsing free-form error text (`AGENTS.md` §12.1).
 */
export function resolveErrorMessage(err: unknown): string {
  if (isAppErrorPayload(err)) {
    const base = getErrorMessage(err.code);
    return err.details ? `${base} (${err.details})` : base;
  }

  if (typeof err === 'string') {
    try {
      const parsed: unknown = JSON.parse(err);
      if (isAppErrorPayload(parsed)) {
        const base = getErrorMessage(parsed.code);
        return parsed.details ? `${base} (${parsed.details})` : base;
      }
    } catch {
      // Fall through to generic fallback below without echoing raw backend strings.
    }
  }

  if (err instanceof Error) {
    try {
      const parsed: unknown = JSON.parse(err.message);
      if (isAppErrorPayload(parsed)) {
        const base = getErrorMessage(parsed.code);
        return parsed.details ? `${base} (${parsed.details})` : base;
      }
    } catch {
      // Not JSON payload; return mapped internal error message.
    }
  }

  return getErrorMessage('INTERNAL_ERROR');
}
