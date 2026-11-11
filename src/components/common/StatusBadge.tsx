import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  FolderCheck,
  FolderMinus,
  FolderX,
  Loader2,
  ShieldAlert,
  SkipForward,
  XCircle,
} from 'lucide-react';
import { messages } from '../../i18n';
import type { BatchStatus, FolderStatus, OperationStatus } from '../../types/generated';

type SemanticTone = 'success' | 'warning' | 'danger' | 'neutral' | 'attention' | 'accent';

const toneClasses: Record<SemanticTone, string> = {
  success: 'border-success-border bg-success-bg text-success-ink',
  warning: 'border-warning-border bg-warning-bg text-warning-ink',
  danger: 'border-danger-border bg-danger-bg text-danger-ink',
  neutral: 'border-neutral-border bg-neutral-bg text-neutral-ink',
  attention: 'border-attention-border bg-attention-bg text-attention-ink',
  accent: 'border-accent-border bg-accent-subtle text-accent',
};

const batchMeta: Record<
  BatchStatus,
  { tone: SemanticTone; label: string; Icon: typeof CheckCircle2 }
> = {
  completed: {
    tone: 'success',
    label: messages.statuses.batch.completed,
    Icon: CheckCircle2,
  },
  partial: {
    tone: 'warning',
    label: messages.statuses.batch.partial,
    Icon: AlertTriangle,
  },
  failed: {
    tone: 'danger',
    label: messages.statuses.batch.failed,
    Icon: XCircle,
  },
  cancelled: {
    tone: 'neutral',
    label: messages.statuses.batch.cancelled,
    Icon: Ban,
  },
  recovery_required: {
    tone: 'attention',
    label: messages.statuses.batch.recovery_required,
    Icon: ShieldAlert,
  },
  running: {
    tone: 'accent',
    label: messages.statuses.batch.running,
    Icon: Loader2,
  },
};

export function BatchStatusBadge({ status }: { status: BatchStatus }) {
  const { tone, label, Icon } = batchMeta[status];
  return (
    <span
      data-status-kind="batch"
      data-status-value={status}
      data-semantic-tone={tone}
      className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs font-medium ${toneClasses[tone]}`}
    >
      <Icon
        className={`h-3.5 w-3.5 shrink-0 ${status === 'running' ? 'animate-spin' : ''}`}
        aria-hidden="true"
      />
      <span>{label}</span>
    </span>
  );
}

const operationMeta: Record<
  OperationStatus,
  { tone: SemanticTone; label: string; Icon: typeof CheckCircle2 }
> = {
  eligible: {
    tone: 'success',
    label: messages.statuses.operation.eligible,
    Icon: CheckCircle2,
  },
  conflict_renamed: {
    tone: 'warning',
    label: messages.statuses.operation.conflict_renamed,
    Icon: AlertTriangle,
  },
  warning: {
    tone: 'warning',
    label: messages.statuses.operation.warning,
    Icon: AlertTriangle,
  },
  skipped: {
    tone: 'neutral',
    label: messages.statuses.operation.skipped,
    Icon: SkipForward,
  },
};

export function OperationStatusBadge({ status }: { status: OperationStatus }) {
  const { tone, label, Icon } = operationMeta[status];
  return (
    <span
      data-status-kind="operation"
      data-status-value={status}
      data-semantic-tone={tone}
      className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs font-medium ${toneClasses[tone]}`}
    >
      <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <span>{label}</span>
    </span>
  );
}

const folderMeta: Record<
  FolderStatus,
  { tone: SemanticTone; label: string; Icon: typeof FolderCheck }
> = {
  ok: {
    tone: 'success',
    label: messages.statuses.folder.ok,
    Icon: FolderCheck,
  },
  unavailable: {
    tone: 'attention',
    label: messages.statuses.folder.unavailable,
    Icon: FolderX,
  },
  ignored: {
    tone: 'neutral',
    label: messages.statuses.folder.ignored,
    Icon: FolderMinus,
  },
  invalid: {
    tone: 'danger',
    label: messages.statuses.folder.invalid,
    Icon: FolderX,
  },
};

export function FolderStatusBadge({ status }: { status: FolderStatus }) {
  const { tone, label, Icon } = folderMeta[status];
  return (
    <span
      data-status-kind="folder"
      data-status-value={status}
      data-semantic-tone={tone}
      className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs font-medium ${toneClasses[tone]}`}
    >
      <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <span>{label}</span>
    </span>
  );
}
