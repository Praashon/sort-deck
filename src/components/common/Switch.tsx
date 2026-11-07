interface SwitchProps {
  id?: string | undefined;
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string | undefined;
  disabled?: boolean | undefined;
  compact?: boolean | undefined;
}

export function Switch({
  id,
  checked,
  onChange,
  label,
  description,
  disabled = false,
  compact = false,
}: SwitchProps) {
  return (
    <div className="flex items-center justify-between gap-3">
      {!compact && (
        <div className="min-w-0">
          <span className="block text-sm font-medium text-ink">{label}</span>
          {description && (
            <span className="mt-0.5 block text-xs leading-relaxed text-ink-muted">
              {description}
            </span>
          )}
        </div>
      )}
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
          checked
            ? 'border-accent bg-accent'
            : 'border-border-strong bg-surface-inset hover:border-ink-muted'
        }`}
      >
        <span
          className={`pointer-events-none inline-block h-3.5 w-3.5 rounded-full transition-transform ${
            checked ? 'translate-x-4 bg-accent-ink' : 'translate-x-0.5 bg-ink-secondary'
          }`}
        />
      </button>
    </div>
  );
}
