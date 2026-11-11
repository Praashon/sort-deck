import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description: string;
  headingLevel?: 'h2' | 'h3' | undefined;
  action?: ReactNode | undefined;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  headingLevel = 'h2',
  action,
}: EmptyStateProps) {
  const Heading = headingLevel;
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border-subtle bg-surface-elevated px-6 py-10 text-center">
      <div className="flex h-10 w-10 items-center justify-center rounded-lg border border-border-subtle bg-surface-inset text-ink-muted">
        <Icon className="h-5 w-5" aria-hidden="true" />
      </div>
      <Heading className="mt-3 text-sm font-medium text-ink">{title}</Heading>
      <p className="mt-1 max-w-md text-xs leading-relaxed text-ink-muted">{description}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
