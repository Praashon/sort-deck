import { useState } from 'react';
import { CheckCircle2, RefreshCw, ShieldAlert } from 'lucide-react';
import { EmptyState } from '../components/common/EmptyState';
import { PathDisplay } from '../components/common/PathDisplay';
import { SkeletonBlock } from '../components/common/SkeletonBlock';
import { BatchStatusBadge } from '../components/common/StatusBadge';
import { ConfirmDialog } from '../components/dialogs/ConfirmDialog';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type { RecoveryEntry } from '../types/generated';
import { formatTimestamp } from '../utils/format';
import { resolveErrorMessage } from '../utils/paths';

export function Recovery() {
  const { loading, recoveryItems, refreshAll } = useAppData();
  const { pushToast } = useToast();

  const [retrying, setRetrying] = useState(false);
  const [ackTarget, setAckTarget] = useState<RecoveryEntry | null>(null);
  const [acking, setAcking] = useState(false);

  const handleRetryRecovery = async () => {
    setRetrying(true);
    try {
      const summary = await ipc.retryRecovery();
      await refreshAll();
      pushToast(
        summary.recoveryRequired > 0 ? 'warning' : 'info',
        `${messages.recovery.retryToastPrefix} ${summary.inspectedEntries} inspected, ${summary.finalizedCompleted} completed, ${summary.recoveryRequired} remaining.`,
      );
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setRetrying(false);
    }
  };

  const handleConfirmAcknowledge = async () => {
    if (!ackTarget) {
      return;
    }
    setAcking(true);
    try {
      await ipc.acknowledgeRecoveryItem(ackTarget.journalId);
      setAckTarget(null);
      await refreshAll();
      pushToast('info', messages.recovery.acknowledgedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setAcking(false);
    }
  };

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle pb-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-ink">
            {messages.recovery.title}
          </h1>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.recovery.subtitle}</p>
        </div>

        <button
          type="button"
          disabled={retrying}
          onClick={() => void handleRetryRecovery()}
          className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
        >
          <RefreshCw
            className={`h-3.5 w-3.5 ${retrying ? 'animate-spin' : ''}`}
            aria-hidden="true"
          />
          <span>{retrying ? messages.recovery.retrying : messages.recovery.retryAllButton}</span>
        </button>
      </header>

      {loading ? (
        <SkeletonBlock rows={3} />
      ) : recoveryItems.length === 0 ? (
        <EmptyState
          icon={CheckCircle2}
          title={messages.recovery.emptyTitle}
          description={messages.recovery.emptyDesc}
        />
      ) : (
        <div className="space-y-3">
          {recoveryItems.map((entry) => (
            <section
              key={entry.journalId}
              aria-label={`Recovery entry ${entry.journalId}`}
              className="rounded-lg border border-attention-border bg-surface-elevated p-5"
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <BatchStatusBadge status="recovery_required" />
                    <span className="text-xs font-medium text-ink">
                      {messages.statuses.operationType[entry.operationType]}
                    </span>
                    <span className="tabular-nums text-[11px] text-ink-muted">
                      #{entry.journalId} · {formatTimestamp(entry.updatedAt)}
                    </span>
                  </div>
                  <p className="font-mono text-[11px] text-ink-muted">{entry.batchId}</p>
                </div>

                <button
                  type="button"
                  onClick={() => setAckTarget(entry)}
                  className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
                >
                  <ShieldAlert className="h-3.5 w-3.5 text-attention-ink" aria-hidden="true" />
                  <span>{messages.recovery.acknowledgeButton}</span>
                </button>
              </div>

              <dl className="mt-4 grid grid-cols-1 gap-3 border-t border-border-subtle pt-3 text-xs md:grid-cols-2">
                <div>
                  <dt className="text-[11px] font-medium text-ink-muted">
                    {messages.recovery.sourceLabel}
                  </dt>
                  <dd className="mt-0.5">
                    <PathDisplay path={entry.sourcePath} maxLength={58} />
                  </dd>
                </div>

                <div>
                  <dt className="text-[11px] font-medium text-ink-muted">
                    {messages.recovery.destinationLabel}
                  </dt>
                  <dd className="mt-0.5">
                    <PathDisplay path={entry.destinationPath} maxLength={58} />
                  </dd>
                </div>

                {entry.temporaryPath && (
                  <div>
                    <dt className="text-[11px] font-medium text-ink-muted">
                      {messages.recovery.tempLabel}
                    </dt>
                    <dd className="mt-0.5">
                      <PathDisplay path={entry.temporaryPath} maxLength={58} />
                    </dd>
                  </div>
                )}

                {(entry.detail || entry.errorCode !== null || entry.errorMessage) && (
                  <div>
                    <dt className="text-[11px] font-medium text-ink-muted">
                      {messages.recovery.detailLabel}
                    </dt>
                    <dd className="mt-0.5 font-mono text-[11px] text-attention-ink">
                      {entry.detail ?? entry.errorMessage ?? ''}
                      {entry.errorCode !== null ? ` (OS ${entry.errorCode})` : ''}
                    </dd>
                  </div>
                )}
              </dl>
            </section>
          ))}
        </div>
      )}

      <ConfirmDialog
        open={ackTarget !== null}
        title={messages.recovery.ackDialogTitle}
        description={messages.recovery.ackDialogDesc}
        willHappen={[messages.recovery.ackWillHappen]}
        willNotHappen={[messages.recovery.ackWillNotHappen]}
        confirmLabel={messages.recovery.acknowledgeButton}
        busy={acking}
        onConfirm={() => void handleConfirmAcknowledge()}
        onCancel={() => setAckTarget(null)}
      />
    </div>
  );
}
