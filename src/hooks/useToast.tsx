import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from 'lucide-react';
import { messages } from '../i18n';

export type ToastVariant = 'success' | 'warning' | 'error' | 'info';

export interface ToastItem {
  id: string;
  variant: ToastVariant;
  message: string;
}

interface ToastContextValue {
  toasts: readonly ToastItem[];
  pushToast: (variant: ToastVariant, message: string) => void;
  dismissToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

const variantStyleMap: Record<ToastVariant, string> = {
  success: 'border-success-border bg-surface-elevated text-ink',
  warning: 'border-warning-border bg-surface-elevated text-ink',
  error: 'border-danger-border bg-surface-elevated text-ink',
  info: 'border-border-strong bg-surface-elevated text-ink',
};

function ToastIcon({ variant }: { variant: ToastVariant }) {
  switch (variant) {
    case 'success':
      return <CheckCircle2 className="h-4 w-4 shrink-0 text-success-ink" aria-hidden="true" />;
    case 'warning':
      return <AlertTriangle className="h-4 w-4 shrink-0 text-warning-ink" aria-hidden="true" />;
    case 'error':
      return <XCircle className="h-4 w-4 shrink-0 text-danger-ink" aria-hidden="true" />;
    case 'info':
      return <Info className="h-4 w-4 shrink-0 text-accent" aria-hidden="true" />;
  }
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const pushToast = useCallback((variant: ToastVariant, message: string) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    setToasts((prev) => [...prev.slice(-3), { id, variant, message }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((item) => item.id !== id));
    }, 5500);
  }, []);

  const value = useMemo(
    () => ({
      toasts,
      pushToast,
      dismissToast,
    }),
    [toasts, pushToast, dismissToast],
  );

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        aria-live="polite"
        aria-atomic="false"
        className="pointer-events-none fixed right-5 bottom-5 z-50 flex w-88 max-w-[calc(100vw-2.5rem)] flex-col gap-2"
      >
        {toasts.map((toast) => (
          <div
            key={toast.id}
            role="status"
            data-toast-variant={toast.variant}
            className={`pointer-events-auto flex items-start gap-2.5 rounded-lg border px-3.5 py-2.5 shadow-sm ${variantStyleMap[toast.variant]}`}
          >
            <div className="pt-0.5">
              <ToastIcon variant={toast.variant} />
            </div>
            <p className="flex-1 text-xs leading-relaxed text-ink">{toast.message}</p>
            <button
              type="button"
              onClick={() => dismissToast(toast.id)}
              aria-label={messages.common.close}
              className="rounded p-0.5 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToast must be used within ToastProvider');
  }
  return ctx;
}
