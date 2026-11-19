import { useEffect, useId, useState, type FormEvent } from 'react';
import { X } from 'lucide-react';
import { messages } from '../../i18n';
import type { Rule } from '../../types/generated';
import { Switch } from '../common/Switch';

export interface RuleFormValues {
  name: string;
  destination: string;
  extensions: string[];
  enabled: boolean;
  nameContains: string | null;
  minSizeBytes: number | null;
  maxSizeBytes: number | null;
  minAgeDays: number | null;
}

interface RuleEditorModalProps {
  open: boolean;
  rule: Rule | null;
  busy: boolean;
  backendError: string | null;
  onSave: (values: RuleFormValues) => Promise<void> | void;
  onClose: () => void;
}

const INVALID_EXT_PATTERN = /[/\\:*?"<>|\s]/;
const BYTES_PER_MB = 1024 * 1024;

export function parseExtensionInput(raw: string): { extensions: string[]; error: string | null } {
  const parts = raw
    .split(/[\s,;]+/)
    .map((part) => part.trim().toLowerCase().replace(/^\./, ''))
    .filter((part) => part.length > 0);

  if (parts.length === 0) {
    return { extensions: [], error: messages.rules.validationExtensionsRequired };
  }

  const seen = new Set<string>();
  const unique: string[] = [];
  for (const ext of parts) {
    if (ext.length > 32) {
      return { extensions: [], error: messages.rules.validationExtensionsRequired };
    }
    if (INVALID_EXT_PATTERN.test(ext)) {
      return { extensions: [], error: messages.rules.validationExtensionInvalidChar };
    }
    if (!seen.has(ext)) {
      seen.add(ext);
      unique.push(ext);
    }
  }
  return { extensions: unique, error: null };
}

function mbFromBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) {
    return '';
  }
  const mb = bytes / BYTES_PER_MB;
  return Number.isInteger(mb) ? String(mb) : mb.toFixed(1);
}

export function RuleEditorModal({
  open,
  rule,
  busy,
  backendError,
  onSave,
  onClose,
}: RuleEditorModalProps) {
  const titleId = useId();
  const nameId = useId();
  const destId = useId();
  const extId = useId();
  const nameContainsId = useId();
  const minSizeId = useId();
  const maxSizeId = useId();
  const minAgeId = useId();

  const [name, setName] = useState(rule?.name ?? '');
  const [destination, setDestination] = useState(rule?.destination ?? '');
  const [extensionsRaw, setExtensionsRaw] = useState(rule?.extensions.join(', ') ?? '');
  const [enabled, setEnabled] = useState(rule?.enabled ?? true);
  const [nameContains, setNameContains] = useState(rule?.nameContains ?? '');
  const [minSizeMb, setMinSizeMb] = useState(mbFromBytes(rule?.minSizeBytes));
  const [maxSizeMb, setMaxSizeMb] = useState(mbFromBytes(rule?.maxSizeBytes));
  const [minAgeDays, setMinAgeDays] = useState(
    rule?.minAgeDays !== null && rule?.minAgeDays !== undefined ? String(rule.minAgeDays) : '',
  );
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [open, busy, onClose]);

  if (!open) {
    return null;
  }

  const appendDateToken = (token: string) => {
    setDestination((prev) => {
      const cleaned = prev.trim();
      if (!cleaned) {
        return `${token}/`;
      }
      return cleaned.endsWith('/') || cleaned.endsWith('\\')
        ? `${cleaned}${token}/`
        : `${cleaned}/${token}/`;
    });
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const trimmedName = name.trim();
    if (trimmedName.length === 0 || trimmedName.length > 80) {
      setValidationError(messages.rules.validationNameRequired);
      return;
    }
    const trimmedDest = destination.trim();
    if (trimmedDest.length === 0) {
      setValidationError(messages.rules.validationDestinationRequired);
      return;
    }
    if (trimmedDest.split(/[/\\]+/).some((seg) => seg === '..')) {
      setValidationError(messages.rules.validationDestinationTraversal);
      return;
    }

    const parsed = parseExtensionInput(extensionsRaw);
    if (parsed.error) {
      setValidationError(parsed.error);
      return;
    }

    const parsedMinMb = minSizeMb.trim() ? Number.parseFloat(minSizeMb.trim()) : null;
    const parsedMaxMb = maxSizeMb.trim() ? Number.parseFloat(maxSizeMb.trim()) : null;
    const minBytes =
      parsedMinMb !== null && Number.isFinite(parsedMinMb) && parsedMinMb >= 0
        ? Math.round(parsedMinMb * BYTES_PER_MB)
        : null;
    const maxBytes =
      parsedMaxMb !== null && Number.isFinite(parsedMaxMb) && parsedMaxMb >= 0
        ? Math.round(parsedMaxMb * BYTES_PER_MB)
        : null;

    if (minBytes !== null && maxBytes !== null && minBytes > maxBytes) {
      setValidationError(messages.rules.validationSizeBounds);
      return;
    }

    const parsedAge = minAgeDays.trim() ? Number.parseInt(minAgeDays.trim(), 10) : null;
    const ageDays =
      parsedAge !== null && !Number.isNaN(parsedAge) && parsedAge >= 0 ? parsedAge : null;

    setValidationError(null);
    void onSave({
      name: trimmedName,
      destination: trimmedDest,
      extensions: parsed.extensions,
      enabled,
      nameContains: nameContains.trim() ? nameContains.trim() : null,
      minSizeBytes: minBytes,
      maxSizeBytes: maxBytes,
      minAgeDays: ageDays,
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-lg border border-border-strong bg-surface-elevated p-5 shadow-xl"
      >
        <div className="flex items-center justify-between gap-3 border-b border-border-subtle pb-3">
          <h2 id={titleId} className="text-sm font-semibold text-ink">
            {rule ? messages.rules.editorEditTitle : messages.rules.editorCreateTitle}
          </h2>
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            aria-label={messages.common.close}
            className="rounded p-1 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          {(validationError || backendError) && (
            <div
              role="alert"
              className="rounded-md border border-danger-border bg-danger-bg px-3 py-2 text-xs text-danger-ink"
            >
              {validationError ?? backendError}
            </div>
          )}

          <div>
            <label htmlFor={nameId} className="block text-xs font-medium text-ink">
              {messages.rules.fieldNameLabel}
            </label>
            <input
              id={nameId}
              type="text"
              maxLength={80}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={messages.rules.fieldNamePlaceholder}
              className="mt-1.5 w-full rounded-md border border-border-subtle bg-surface-inset px-3 py-2 text-xs text-ink placeholder:text-ink-muted"
            />
          </div>

          <div>
            <label htmlFor={destId} className="block text-xs font-medium text-ink">
              {messages.rules.fieldDestinationLabel}
            </label>
            <input
              id={destId}
              type="text"
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
              placeholder={messages.rules.fieldDestinationPlaceholder}
              className="mt-1.5 w-full rounded-md border border-border-subtle bg-surface-inset px-3 py-2 font-mono text-xs text-ink placeholder:text-ink-muted"
            />
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px] text-ink-muted">
              <span>{messages.rules.dateTokensLabel}</span>
              {[
                messages.rules.dateTokenYear,
                messages.rules.dateTokenMonth,
                messages.rules.dateTokenDay,
              ].map((token) => (
                <button
                  key={token}
                  type="button"
                  onClick={() => appendDateToken(token)}
                  className="rounded border border-border-subtle bg-surface px-1.5 py-0.5 font-mono text-[11px] text-ink-secondary hover:bg-surface-hover hover:text-ink"
                >
                  {token}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label htmlFor={extId} className="block text-xs font-medium text-ink">
              {messages.rules.fieldExtensionsLabel}
            </label>
            <input
              id={extId}
              type="text"
              value={extensionsRaw}
              onChange={(e) => setExtensionsRaw(e.target.value)}
              placeholder={messages.rules.fieldExtensionsPlaceholder}
              className="mt-1.5 w-full rounded-md border border-border-subtle bg-surface-inset px-3 py-2 font-mono text-xs text-ink placeholder:text-ink-muted"
            />
          </div>

          <div className="space-y-3 rounded-md border border-border-subtle bg-surface-inset p-3">
            <div>
              <p className="text-xs font-medium text-ink">{messages.rules.extraFiltersTitle}</p>
              <p className="text-[11px] text-ink-muted">{messages.rules.extraFiltersDesc}</p>
            </div>

            <div>
              <label htmlFor={nameContainsId} className="block text-[11px] font-medium text-ink">
                {messages.rules.fieldNameContainsLabel}
              </label>
              <input
                id={nameContainsId}
                type="text"
                maxLength={80}
                value={nameContains}
                onChange={(e) => setNameContains(e.target.value)}
                placeholder={messages.rules.fieldNameContainsPlaceholder}
                className="mt-1 w-full rounded border border-border-subtle bg-surface px-2.5 py-1.5 text-xs text-ink placeholder:text-ink-muted"
              />
            </div>

            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
              <div>
                <label htmlFor={minSizeId} className="block text-[11px] font-medium text-ink">
                  {messages.rules.fieldMinSizeMbLabel}
                </label>
                <input
                  id={minSizeId}
                  type="number"
                  min={0}
                  step="any"
                  value={minSizeMb}
                  onChange={(e) => setMinSizeMb(e.target.value)}
                  className="tabular-nums mt-1 w-full rounded border border-border-subtle bg-surface px-2.5 py-1.5 text-xs text-ink"
                />
              </div>

              <div>
                <label htmlFor={maxSizeId} className="block text-[11px] font-medium text-ink">
                  {messages.rules.fieldMaxSizeMbLabel}
                </label>
                <input
                  id={maxSizeId}
                  type="number"
                  min={0}
                  step="any"
                  value={maxSizeMb}
                  onChange={(e) => setMaxSizeMb(e.target.value)}
                  className="tabular-nums mt-1 w-full rounded border border-border-subtle bg-surface px-2.5 py-1.5 text-xs text-ink"
                />
              </div>

              <div>
                <label htmlFor={minAgeId} className="block text-[11px] font-medium text-ink">
                  {messages.rules.fieldMinAgeDaysLabel}
                </label>
                <input
                  id={minAgeId}
                  type="number"
                  min={0}
                  max={36500}
                  value={minAgeDays}
                  onChange={(e) => setMinAgeDays(e.target.value)}
                  className="tabular-nums mt-1 w-full rounded border border-border-subtle bg-surface px-2.5 py-1.5 text-xs text-ink"
                />
              </div>
            </div>
          </div>

          <div className="rounded-md border border-border-subtle bg-surface-inset px-3 py-2.5">
            <Switch
              checked={enabled}
              onChange={setEnabled}
              label={messages.rules.fieldEnabledLabel}
            />
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border-subtle pt-4">
            <button
              type="button"
              onClick={onClose}
              disabled={busy}
              className="rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
            >
              {messages.common.cancel}
            </button>
            <button
              type="submit"
              disabled={busy}
              className="rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
            >
              {busy ? messages.common.saving : messages.common.save}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
