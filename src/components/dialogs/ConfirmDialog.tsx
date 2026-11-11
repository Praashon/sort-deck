import { useEffect, useId, useRef } from 'react';
import { AlertTriangle, X } from 'lucide-react';
import { messages } from '../../i18n';

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  willHappen: readonly string[];
  willNotHappen: readonly string[];
  confirmLabel: string;
  cancelLabel?: string | undefined;
  destructive?: boolean | undefined;
  busy?: boolean | undefined;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  description,
  willHappen,
  willNotHappen,
  confirmLabel,
  cancelLabel = messages.common.cancel,
  destructive = false,
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const titleId = useId();
  const descId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) {
      return undefined;
    }

    const previousFocused = document.activeElement as HTMLElement | null;
    cancelBtnRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) {
        event.preventDefault();
        onCancel();
        return;
      }

      if (event.key !== 'Tab' || !dialogRef.current) {
        return;
      }

      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) {
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) {
        return;
      }

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      if (previousFocused && typeof previousFocused.focus === 'function') {
        previousFocused.focus();
      }
    };
  }, [open, busy, onCancel]);

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        className="w-full max-w-md rounded-lg border border-border-strong bg-surface-elevated p-5 shadow-lg"
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-2.5">
            {destructive && (
              <div className="mt-0.5 rounded-md border border-danger-border bg-danger-bg p-1.5 text-danger-ink">
                <AlertTriangle className="h-4 w-4" aria-hidden="true" />
              </div>
            )}
            <div>
              <h2 id={titleId} className="text-base font-semibold text-ink">
                {title}
              </h2>
              <p id={descId} className="mt-1 text-xs leading-relaxed text-ink-muted">
                {description}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onCancel}
            disabled={busy}
            aria-label={messages.common.close}
            className="rounded p-1 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink disabled:opacity-50"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <div className="mt-4 space-y-3 rounded-md border border-border-subtle bg-surface-inset p-3 text-xs">
          <div>
            <p className="font-medium text-ink">{messages.common.willHappenTitle}</p>
            <ul className="mt-1 list-disc space-y-1 pl-4 text-ink-secondary">
              {willHappen.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          </div>
          <div className="border-t border-border-subtle pt-2.5">
            <p className="font-medium text-ink">{messages.common.willNotHappenTitle}</p>
            <ul className="mt-1 list-disc space-y-1 pl-4 text-ink-muted">
              {willNotHappen.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          </div>
        </div>

        <div className="mt-5 flex items-center justify-end gap-2">
          <button
            ref={cancelBtnRef}
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className={`rounded-md px-3.5 py-1.5 text-xs font-medium transition-colors disabled:opacity-50 ${
              destructive
                ? 'border border-danger-border bg-danger-bg text-danger-ink hover:opacity-90'
                : 'bg-primary text-primary-ink hover:bg-primary-hover'
            }`}
          >
            {busy ? messages.common.saving : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
