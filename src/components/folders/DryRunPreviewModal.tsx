import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { observeElementRect, useVirtualizer } from '@tanstack/react-virtual';
import {
  AlertTriangle,
  ArrowRight,
  Check,
  Loader2,
  Play,
  Search,
  ShieldAlert,
  X,
} from 'lucide-react';
import { Link } from 'react-router';
import { messages } from '../../i18n';
import * as ipc from '../../services/ipc';
import type {
  ExecutionResult,
  OperationStatus,
  PlannedOperation,
  PlanPageResponse,
  PlanSummary,
  SkipReason,
} from '../../types/generated';
import { formatBytes, formatNumber } from '../../utils/format';
import { resolveErrorMessage, sanitizeFilenameForDisplay } from '../../utils/paths';
import { PathDisplay } from '../common/PathDisplay';
import { SegmentedControl } from '../common/SegmentedControl';
import { BatchStatusBadge, OperationStatusBadge } from '../common/StatusBadge';
import { ConfirmDialog } from '../dialogs/ConfirmDialog';

interface DryRunPreviewModalProps {
  plan: PlanSummary;
  onClose: () => void;
  onExecuted: (result: ExecutionResult) => Promise<void> | void;
}

type StatusFilter = 'all' | 'eligible' | 'conflict_renamed' | 'skipped';

const PAGE_LIMIT = 200;

export function DryRunPreviewModal({ plan, onClose, onExecuted }: DryRunPreviewModalProps) {
  const titleId = useId();
  const scrollRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [destinationFilter, setDestinationFilter] = useState<string>('all');
  const [skipReasonFilter, setSkipReasonFilter] = useState<SkipReason | 'all'>('all');
  const [search, setSearch] = useState('');
  const [offset, setOffset] = useState(0);

  const [pageData, setPageData] = useState<PlanPageResponse | null>(null);
  const [loadingPage, setLoadingPage] = useState(true);
  const [pageError, setPageError] = useState<string | null>(null);

  const [deselectedIds, setDeselectedIds] = useState<Set<string>>(() => new Set());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [execError, setExecError] = useState<string | null>(null);
  const [execResult, setExecResult] = useState<ExecutionResult | null>(null);

  const effectiveStatusFilter: OperationStatus | null = useMemo(() => {
    if (statusFilter === 'all') {
      return null;
    }
    return statusFilter;
  }, [statusFilter]);

  const loadPage = useCallback(async () => {
    setLoadingPage(true);
    setPageError(null);
    try {
      const response = await ipc.getPlanPage({
        planId: plan.planId,
        offset,
        limit: PAGE_LIMIT,
        statusFilter: effectiveStatusFilter,
        skipReasonFilter: skipReasonFilter === 'all' ? null : skipReasonFilter,
        search: search.trim().length > 0 ? search.trim() : null,
      });
      setPageData(response);
    } catch (err) {
      setPageError(resolveErrorMessage(err));
    } finally {
      setLoadingPage(false);
    }
  }, [plan.planId, offset, effectiveStatusFilter, skipReasonFilter, search]);

  useEffect(() => {
    void loadPage();
  }, [loadPage]);

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !confirmOpen && !executing) {
        e.preventDefault();
        if (!execResult) {
          void ipc.discardPlan(plan.planId).catch(() => undefined);
        }
        onClose();
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [confirmOpen, executing, execResult, onClose, plan.planId]);

  const visibleItems: PlannedOperation[] = useMemo(() => {
    const base = pageData?.items ?? [];
    if (destinationFilter === 'all') {
      return base;
    }
    return base.filter((op) => {
      if (!op.destinationPath) {
        return false;
      }
      const normDst = op.destinationPath.replace(/\\/g, '/').toLowerCase();
      const normGroup = destinationFilter.replace(/\\/g, '/').toLowerCase();
      return normDst.startsWith(normGroup) || normDst.includes(`/${normGroup}`);
    });
  }, [pageData, destinationFilter]);

  const rowVirtualizer = useVirtualizer({
    count: visibleItems.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 56,
    overscan: 8,
    initialRect: { width: 800, height: 380 },
    observeElementRect: (instance, cb) =>
      observeElementRect(instance, (rect) => {
        cb(rect.height === 0 && rect.width === 0 ? { width: 800, height: 380 } : rect);
      }),
  });

  const handleCloseModal = () => {
    if (!execResult) {
      void ipc.discardPlan(plan.planId).catch(() => undefined);
    }
    onClose();
  };

  const toggleOperationSelection = (opId: string) => {
    setDeselectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(opId)) {
        next.delete(opId);
      } else {
        next.add(opId);
      }
      return next;
    });
  };

  const handleIncludeAllVisible = () => {
    setDeselectedIds((prev) => {
      const next = new Set(prev);
      for (const item of visibleItems) {
        next.delete(item.operationId);
      }
      return next;
    });
  };

  const actionableTotal = plan.eligibleCount;
  const selectedEstimate = Math.max(0, actionableTotal - deselectedIds.size);

  const handleExecuteConfirmed = async () => {
    setConfirmOpen(false);
    setExecuting(true);
    setExecError(null);
    try {
      const result = await ipc.executePlan({
        planId: plan.planId,
        deselectedOperationIds: Array.from(deselectedIds),
      });
      setExecResult(result);
      await onExecuted(result);
    } catch (err) {
      setExecError(resolveErrorMessage(err));
    } finally {
      setExecuting(false);
    }
  };

  const getResultHeadline = (result: ExecutionResult): string => {
    switch (result.status) {
      case 'completed':
        return messages.preview.resultTitleCompleted;
      case 'partial':
        return messages.preview.resultTitlePartial;
      case 'failed':
        return messages.preview.resultTitleFailed;
      case 'cancelled':
        return messages.preview.resultTitleCancelled;
      case 'recovery_required':
        return messages.preview.resultTitleRecoveryRequired;
      case 'running':
        return messages.statuses.batch.running;
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/65 p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="flex h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-border-strong bg-surface-elevated shadow-xl"
      >
        <div className="flex items-center justify-between gap-4 border-b border-border-subtle px-5 py-3.5">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h2 id={titleId} className="text-sm font-semibold text-ink">
                {messages.preview.title}
              </h2>
              {plan.sortignoreDetected && (
                <span className="rounded border border-border-strong bg-surface-inset px-2 py-0.5 text-[11px] font-medium text-ink-secondary">
                  {messages.preview.sortignoreDetectedBadge}
                </span>
              )}
            </div>
            <div className="mt-0.5">
              <PathDisplay path={plan.folderPath} maxLength={78} />
            </div>
          </div>

          <button
            type="button"
            onClick={handleCloseModal}
            disabled={executing}
            aria-label={messages.preview.closeAriaLabel}
            className="rounded p-1.5 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink disabled:opacity-50"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <div className="grid grid-cols-2 divide-y divide-border-subtle border-b border-border-subtle bg-surface sm:grid-cols-6 sm:divide-x sm:divide-y-0">
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statDiscovered}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-ink">
              {formatNumber(plan.totalDiscovered)}
            </p>
          </div>
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statEligible}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-accent">
              {formatNumber(plan.eligibleCount)}
            </p>
          </div>
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statRenamed}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-warning-ink">
              {formatNumber(plan.conflictRenamedCount)}
            </p>
          </div>
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statSkipped}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-ink-secondary">
              {formatNumber(plan.skippedCount)}
            </p>
          </div>
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statTotalBytes}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-ink">
              {formatBytes(plan.totalBytesToMove)}
            </p>
          </div>
          <div className="px-4 py-2.5">
            <p className="text-[11px] text-ink-muted">{messages.preview.statCrossVolume}</p>
            <p className="tabular-nums mt-0.5 text-base font-semibold text-ink">
              {formatNumber(plan.crossVolumeCount)}
            </p>
          </div>
        </div>

        {plan.warnings.length > 0 && (
          <div className="border-b border-warning-border bg-warning-bg px-5 py-2 text-xs text-warning-ink">
            <div className="flex items-start gap-2">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <ul className="space-y-0.5">
                {plan.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            </div>
          </div>
        )}

        {execResult && (
          <div
            role="status"
            className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle bg-surface-inset px-5 py-3"
          >
            <div className="flex items-center gap-3">
              <BatchStatusBadge status={execResult.status} />
              <div>
                <p className="text-xs font-medium text-ink">{getResultHeadline(execResult)}</p>
                <p className="tabular-nums text-[11px] text-ink-muted">
                  {execResult.succeeded} {messages.dashboard.filesMovedLabel} · {execResult.failed}{' '}
                  {messages.dashboard.filesFailedLabel} · {execResult.skipped}{' '}
                  {messages.dashboard.filesSkippedLabel}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              {execResult.recoveryRequiredCount > 0 && (
                <Link
                  to="/recovery"
                  onClick={onClose}
                  className="inline-flex items-center gap-1 rounded border border-attention-border bg-attention-bg px-2.5 py-1 text-xs font-medium text-attention-ink"
                >
                  <ShieldAlert className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{messages.preview.openRecovery}</span>
                </Link>
              )}
              <Link
                to="/history"
                onClick={onClose}
                className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface px-2.5 py-1 text-xs font-medium text-ink hover:bg-surface-hover"
              >
                <span>{messages.preview.viewBatchHistory}</span>
                <ArrowRight className="h-3 w-3" aria-hidden="true" />
              </Link>
            </div>
          </div>
        )}

        {execError && (
          <div
            role="alert"
            className="border-b border-danger-border bg-danger-bg px-5 py-2.5 text-xs text-danger-ink"
          >
            {execError}
          </div>
        )}

        {!execResult && (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle px-5 py-3">
              <SegmentedControl
                ariaLabel={messages.preview.columnStatus}
                value={statusFilter}
                onChange={(next) => {
                  setStatusFilter(next);
                  setOffset(0);
                }}
                options={[
                  { value: 'all', label: messages.preview.filterAll, count: plan.totalDiscovered },
                  {
                    value: 'eligible',
                    label: messages.preview.filterEligible,
                    count: plan.eligibleCount,
                  },
                  {
                    value: 'conflict_renamed',
                    label: messages.preview.filterRenamed,
                    count: plan.conflictRenamedCount,
                  },
                  {
                    value: 'skipped',
                    label: messages.preview.filterSkipped,
                    count: plan.skippedCount,
                  },
                ]}
              />

              <div className="flex flex-wrap items-center gap-2">
                {plan.destinationGroups.length > 0 && (
                  <select
                    aria-label={messages.preview.allDestinations}
                    value={destinationFilter}
                    onChange={(e) => setDestinationFilter(e.target.value)}
                    className="rounded-md border border-border-subtle bg-surface-inset px-2.5 py-1.5 text-xs text-ink"
                  >
                    <option value="all">{messages.preview.allDestinations}</option>
                    {plan.destinationGroups.map((grp) => (
                      <option key={grp.destinationDir} value={grp.destinationDir}>
                        {sanitizeFilenameForDisplay(grp.destinationDir)} ({grp.fileCount})
                      </option>
                    ))}
                  </select>
                )}

                {plan.skipReasonCounts.length > 0 && (
                  <select
                    aria-label={messages.preview.allSkipReasons}
                    value={skipReasonFilter}
                    onChange={(e) => {
                      setSkipReasonFilter(e.target.value as SkipReason | 'all');
                      setOffset(0);
                    }}
                    className="rounded-md border border-border-subtle bg-surface-inset px-2.5 py-1.5 text-xs text-ink"
                  >
                    <option value="all">{messages.preview.allSkipReasons}</option>
                    {plan.skipReasonCounts.map((entry) => (
                      <option key={entry.reason} value={entry.reason}>
                        {entry.reason} ({entry.count})
                      </option>
                    ))}
                  </select>
                )}

                <div className="relative">
                  <Search
                    className="pointer-events-none absolute top-2 left-2.5 h-3.5 w-3.5 text-ink-muted"
                    aria-hidden="true"
                  />
                  <input
                    type="search"
                    aria-label={messages.preview.searchPlaceholder}
                    placeholder={messages.preview.searchPlaceholder}
                    value={search}
                    onChange={(e) => {
                      setSearch(e.target.value);
                      setOffset(0);
                    }}
                    className="w-52 rounded-md border border-border-subtle bg-surface-inset py-1.5 pr-2.5 pl-8 text-xs text-ink placeholder:text-ink-muted"
                  />
                </div>
              </div>
            </div>

            <div className="grid grid-cols-[56px_minmax(0,1.2fr)_minmax(0,1.5fr)_84px_124px] items-center gap-3 border-b border-border-subtle bg-surface-inset px-5 py-2 text-[11px] font-medium text-ink-muted">
              <span>{messages.preview.columnInclude}</span>
              <span>{messages.preview.columnFile}</span>
              <span>{messages.preview.columnDestination}</span>
              <span className="text-right">{messages.preview.columnSize}</span>
              <span className="text-right">{messages.preview.columnStatus}</span>
            </div>

            <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
              {loadingPage && (
                <div className="flex items-center justify-center gap-2 py-12 text-xs text-ink-muted">
                  <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                  <span>{messages.preview.loadingRows}</span>
                </div>
              )}

              {!loadingPage && pageError && (
                <div className="p-6 text-center text-xs text-danger-ink">{pageError}</div>
              )}

              {!loadingPage && !pageError && visibleItems.length === 0 && (
                <div className="p-8 text-center text-xs text-ink-muted">
                  {messages.preview.emptyRows}
                </div>
              )}

              {!loadingPage && !pageError && visibleItems.length > 0 && (
                <div
                  style={{
                    height: `${rowVirtualizer.getTotalSize()}px`,
                    position: 'relative',
                    width: '100%',
                  }}
                >
                  {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                    const op = visibleItems[virtualRow.index];
                    if (!op) {
                      return null;
                    }
                    const canToggle = op.operationStatus !== 'skipped';
                    const isIncluded = canToggle && !deselectedIds.has(op.operationId);
                    const displayOriginal = sanitizeFilenameForDisplay(op.originalFilename);
                    const displayProposed = op.proposedFilename
                      ? sanitizeFilenameForDisplay(op.proposedFilename)
                      : null;

                    return (
                      <div
                        key={op.operationId}
                        style={{
                          position: 'absolute',
                          top: 0,
                          left: 0,
                          width: '100%',
                          height: `${virtualRow.size}px`,
                          transform: `translateY(${virtualRow.start}px)`,
                        }}
                        className="grid grid-cols-[56px_minmax(0,1.2fr)_minmax(0,1.5fr)_84px_124px] items-center gap-3 border-b border-border-subtle px-5 text-xs hover:bg-surface-hover"
                      >
                        <div>
                          <input
                            type="checkbox"
                            disabled={!canToggle}
                            checked={isIncluded}
                            aria-label={`${messages.preview.columnInclude}: ${displayOriginal}`}
                            onChange={() => toggleOperationSelection(op.operationId)}
                            className="h-3.5 w-3.5 rounded border-border-strong accent-accent disabled:opacity-40"
                          />
                        </div>

                        <div className="min-w-0">
                          <p className="truncate font-medium text-ink" title={displayOriginal}>
                            {displayOriginal}
                          </p>
                          <p className="truncate font-mono text-[11px] text-ink-muted">
                            {op.matchedRuleName ?? messages.common.none}
                          </p>
                        </div>

                        <div className="min-w-0">
                          {op.destinationPath ? (
                            <div className="min-w-0">
                              <PathDisplay
                                path={op.destinationPath}
                                maxLength={54}
                                showCopy={false}
                              />
                              <div className="mt-0.5 flex items-center gap-2 text-[11px] text-ink-muted">
                                {displayProposed && displayProposed !== displayOriginal && (
                                  <span className="text-warning-ink">{displayProposed}</span>
                                )}
                                {op.crossVolumeEstimate && (
                                  <span>{messages.preview.crossVolumeTag}</span>
                                )}
                              </div>
                            </div>
                          ) : (
                            <p className="truncate text-[11px] text-ink-muted">
                              {op.skipReason ? messages.skipReasons[op.skipReason] : ''}
                            </p>
                          )}
                        </div>

                        <div className="tabular-nums text-right text-[11px] text-ink-secondary">
                          {formatBytes(op.fileSize)}
                        </div>

                        <div className="flex justify-end">
                          <OperationStatusBadge status={op.operationStatus} />
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </>
        )}

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border-subtle bg-surface px-5 py-3">
          <div className="flex items-center gap-3 text-xs text-ink-muted">
            {pageData && pageData.totalMatching > PAGE_LIMIT && !execResult && (
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  disabled={offset === 0 || loadingPage}
                  onClick={() => setOffset((prev) => Math.max(0, prev - PAGE_LIMIT))}
                  className="rounded border border-border-subtle bg-surface-elevated px-2 py-1 text-[11px] font-medium text-ink disabled:opacity-40"
                >
                  {messages.common.previous}
                </button>
                <span className="tabular-nums text-[11px]">
                  {offset + 1}–{Math.min(offset + PAGE_LIMIT, pageData.totalMatching)} /{' '}
                  {pageData.totalMatching}
                </span>
                <button
                  type="button"
                  disabled={offset + PAGE_LIMIT >= pageData.totalMatching || loadingPage}
                  onClick={() => setOffset((prev) => prev + PAGE_LIMIT)}
                  className="rounded border border-border-subtle bg-surface-elevated px-2 py-1 text-[11px] font-medium text-ink disabled:opacity-40"
                >
                  {messages.common.next}
                </button>
              </div>
            )}

            {deselectedIds.size > 0 && !execResult && (
              <div className="flex items-center gap-2">
                <span className="tabular-nums">
                  {deselectedIds.size} {messages.preview.deselectedSummary}
                </span>
                <button
                  type="button"
                  onClick={handleIncludeAllVisible}
                  className="text-accent hover:underline"
                >
                  {messages.preview.selectAllVisible}
                </button>
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleCloseModal}
              disabled={executing}
              className="rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
            >
              {messages.common.close}
            </button>

            {!execResult && (
              <button
                type="button"
                disabled={executing || selectedEstimate === 0}
                onClick={() => setConfirmOpen(true)}
                className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
              >
                {executing ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                ) : (
                  <Play className="h-3.5 w-3.5" aria-hidden="true" />
                )}
                <span>
                  {executing
                    ? messages.preview.executing
                    : `${messages.preview.confirmSortButton} (${formatNumber(selectedEstimate)})`}
                </span>
              </button>
            )}

            {execResult && (
              <button
                type="button"
                onClick={onClose}
                className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink hover:bg-primary-hover"
              >
                <Check className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{messages.common.close}</span>
              </button>
            )}
          </div>
        </div>
      </div>

      <ConfirmDialog
        open={confirmOpen}
        title={messages.preview.confirmDialogTitle}
        description={messages.preview.confirmDialogDesc}
        willHappen={[
          `${messages.preview.confirmWillHappenPrefix} ${formatNumber(selectedEstimate)}.`,
          messages.preview.confirmWillHappenSafety,
        ]}
        willNotHappen={[messages.preview.confirmWillNotHappen]}
        confirmLabel={messages.preview.confirmSortButton}
        busy={executing}
        onConfirm={() => void handleExecuteConfirmed()}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  );
}
