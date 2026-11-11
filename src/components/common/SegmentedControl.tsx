export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  count?: number | undefined;
}

interface SegmentedControlProps<T extends string> {
  ariaLabel: string;
  value: T;
  options: readonly SegmentOption<T>[];
  onChange: (value: T) => void;
}

export function SegmentedControl<T extends string>({
  ariaLabel,
  value,
  options,
  onChange,
}: SegmentedControlProps<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className="inline-flex flex-wrap items-center gap-0.5 rounded-lg border border-border-subtle bg-surface-inset p-0.5"
    >
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(option.value)}
            className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
              selected
                ? 'bg-surface-elevated text-ink shadow-xs ring-1 ring-border-subtle'
                : 'text-ink-muted hover:text-ink'
            }`}
          >
            <span>{option.label}</span>
            {option.count !== undefined && (
              <span className="tabular-nums text-[11px] text-ink-muted">{option.count}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
