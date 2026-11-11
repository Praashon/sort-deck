import { en } from './en';
import type { SkipReason } from '../types/generated';

export const messages = en;

export function getErrorMessage(code: string): string {
  const table: Record<string, string> = messages.errors;
  return table[code] ?? messages.errors.INTERNAL_ERROR;
}

export function getSkipReasonMessage(reason: SkipReason): string {
  return messages.skipReasons[reason];
}
