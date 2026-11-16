import { useCallback, useEffect, useRef, useState } from 'react';
import { observeElementRect, useVirtualizer } from '@tanstack/react-virtual';
import { Download, ExternalLink, History as HistoryIcon, RotateCcw } from 'lucide-react';
import { EmptyState } from '../components/common/EmptyState';
import { PathDisplay } from '../components/common/PathDisplay';
import { SegmentedControl } from '../components/common/SegmentedControl';
import { SkeletonBlock } from '../components/common/SkeletonBlock';
import { BatchStatusBadge } from '../components/common/StatusBadge';
import { ConfirmDialog } from '../components/dialogs/ConfirmDialog';
import { MovedFileSearch } from '../components/history/MovedFileSearch';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type {
  BatchDetailResponse,
  BatchStatus,
  BatchSummary,
  MoveLogItem,
} from '../types/generated';
import { buildBatchesCsv, downloadTextFile, formatBytes, formatTimestamp } from '../utils/format';
import { resolveErrorMessage } from '../utils/paths';

type BatchFilter = 'all' | Exclude<BatchStatus, 'running'>;
const BATCH_PAGE_LIMIT = 50;

export function History() {
  const { isDesktopConnected, refreshAll } = useAppData();
  const { pushToast } = useToast();
  const detailScrollRef = useRef<HTMLDivElement>(null);

  const [statusFilter, setStatusFilter] = useState<BatchFilter>('all');
  const [offset, setOffset] = useState(0);
  const [total, setTotal] = useState(0);
  const [batches, setBatches] = useState<BatchSummary[]>([]);
  const [loadingList, setLoadingList] = useState(isDesktopConnected);

  const [selectedBatchId, setSelectedBatchId] = useState<string | null>(null);
  const [detail, setDetail] = useState<BatchDetailResponse | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);

  const [undoTarget, setUndoTarget] = useState<{ batchId: string; itemId: number | null } | null>(
    null,
  );
  const [undoing, setUndoing] = useState(false);

  const loadBatches = useCallback(async () => {
    if (!isDesktopConnected) {
      setLoadingList(false);
      return;
    }
    setLoadingList(true);
    try {
      const res = await ipc.listBatches({
        offset,
        limit: BATCH_PAGE_LIMIT,
        statusFilter: statusFilter === 'all' ? null : statusFilter,
      });
      setBatches(res.items);
      setTotal(res.total);
      setSelectedBatchId((prev) => {
        if (prev && res.items.some((b) => b.id === prev)) {
          return prev;
        }
        return res.items[0]?.id ?? null;
      });
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setLoadingList(false);
    }
  }, [isDesktopConnected, offset, statusFilter, pushToast]);

  useEffect(() => {
    void loadBatches();
  }, [loadBatches]);

  const loadDetail = useCallback(
    async (batchId: string | null) => {
      if (!isDesktopConnected || !batchId) {
        setDetail(null);
        return;
      }
      setLoadingDetail(true);
      try {
        const res = await ipc.getBatchDetail(batchId);
        setDetail(res);
      } catch (err) {
        pushToast('error', resolveErrorMessage(err));
      } finally {
        setLoadingDetail(false);
      }
    },
    [isDesktopConnected, pushToast],
  );

  useEffect(() => {
    void loadDetail(selectedBatchId);
  }, [selectedBatchId, loadDetail]);

  const items: MoveLogItem[] = detail?.items ?? [];
  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => detailScrollRef.current,
    estimateSize: () => 64,
    overscan: 6,
    initialRect: { width: 640, height: 360 },
    observeElementRect: (instance, cb) =>
      observeElementRect(instance, (rect) => {
        cb(rect.height === 0 && rect.width === 0 ? { width: 640, height: 360 } : rect);
      }),
  });

  const handleConfirmUndo = async () => {
    if (!undoTarget) {
      return;
    }
    setUndoing(true);
    try {
      const result = await ipc.undoBatch({
        batchId: undoTarget.batchId,
        itemIds: undoTarget.itemId !== null ? [undoTarget.itemId] : null,
      });
      setUndoTarget(null);
      if (result.status === 'completed') {
        pushToast('success', messages.history.undoCompletedToast);
      } else if (result.status === 'partial') {
        pushToast('warning', messages.history.undoPartialToast);
      } else {
        pushToast('error', messages.history.undoFailedToast);
      }
      await Promise.all([loadBatches(), loadDetail(undoTarget.batchId), refreshAll()]);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setUndoing(false);
    }
  };

  const handleRevealInExplorer = async (moveLogId: number) => {
    try {
      await ipc.revealMoveLogInExplorer(moveLogId);
    } catch (err) {
      pushToast('error', `${messages.history.revealFailedPrefix} ${resolveErrorMessage(err)}`);
    }
  };

  const handleExportJson = () => {
    const payload = JSON.stringify({ batches, selectedBatch: detail }, null, 2);
    downloadTextFile('sortdeck-history.json', payload, 'application/json');
    pushToast('info', messages.history.exportedToast);
  };

  const handleExportCsv = () => {
    const csv = buildBatchesCsv(batches, detail);
    downloadTextFile('sortdeck-history.csv', csv, 'text/csv;charset=utf-8');
    pushToast('info', messages.history.exportedToast);
  };

  const hasUndoableItems =
    detail !== null &&
    detail.batch.kind !== 'undo' &&
    detail.batch.status !== 'running' &&
    detail.items.some((item) => !item.undone);

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle pb-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-ink">
            {messages.history.title}
          </h1>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.history.subtitle}</p>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            disabled={batches.length === 0}
            onClick={handleExportCsv}
            className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-40"
          >
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.history.exportCsv}</span>
          </button>
          <button
            type="button"
            disabled={batches.length === 0}
            onClick={handleExportJson}
            className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-40"
          >
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.history.exportJson}</span>
          </button>
        </div>
      </header>

      <MovedFileSearch />

      <div className="flex flex-wrap items-center justify-between gap-3">
        <SegmentedControl
          ariaLabel={messages.history.batchListTitle}
          value={statusFilter}
          onChange={(next) => {
            setStatusFilter(next);
            setOffset(0);
          }}
          options={[
            { value: 'all', label: messages.history.filterAll },
            { value: 'completed', label: messages.statuses.batch.completed },
            { value: 'partial', label: messages.statuses.batch.partial },
            { value: 'failed', label: messages.statuses.batch.failed },
            { value: 'cancelled', label: messages.statuses.batch.cancelled },
            { value: 'recovery_required', label: messages.statuses.batch.recovery_required },
          ]}
        />

        {total > BATCH_PAGE_LIMIT && (
          <div className="flex items-center gap-2 text-xs text-ink-muted">
            <button
              type="button"
              disabled={offset === 0}
              onClick={() => setOffset((p) => Math.max(0, p - BATCH_PAGE_LIMIT))}
              className="rounded border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs font-medium text-ink disabled:opacity-40"
            >
              {messages.common.previous}
            </button>
            <span className="tabular-nums">
              {offset + 1}–{Math.min(offset + BATCH_PAGE_LIMIT, total)} / {total}
            </span>
            <button
              type="button"
              disabled={offset + BATCH_PAGE_LIMIT >= total}
              onClick={() => setOffset((p) => p + BATCH_PAGE_LIMIT)}
              className="rounded border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs font-medium text-ink disabled:opacity-40"
            >
              {messages.common.next}
            </button>
          </div>
        )}
      </div>

      {loadingList ? (
        <SkeletonBlock rows={5} />
      ) : batches.length === 0 ? (
        <EmptyState
          icon={HistoryIcon}
          title={messages.history.emptyBatchesTitle}
          description={messages.history.emptyBatchesDesc}
        />
      ) : (
        <div className="grid grid-cols-1 gap-5 lg:grid-cols-[340px_minmax(0,1fr)]">
          <div className="divide-y divide-border-subtle overflow-hidden rounded-lg border border-border-subtle bg-surface-elevated">
            {batches.map((batch) => {
              const selected = batch.id === selectedBatchId;
              return (
                <button
                  key={batch.id}
                  type="button"
                  aria-current={selected ? 'true' : undefined}
                  onClick={() => setSelectedBatchId(batch.id)}
                  className={`block w-full px-4 py-3 text-left transition-colors ${
                    selected ? 'bg-accent-subtle' : 'hover:bg-surface-hover'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <BatchStatusBadge status={batch.status} />
                    <span className="tabular-nums text-[11px] text-ink-muted">
                      {formatTimestamp(batch.createdAt)}
                    </span>
                  </div>
                  <p className="mt-2 text-xs font-medium text-ink">
                    {messages.statuses.batchKind[batch.kind]}
                  </p>
                  <p className="mt-0.5 truncate font-mono text-[11px] text-ink-muted">
                    {batch.sourceFolder}
                  </p>
                  <p className="tabular-nums mt-1.5 text-[11px] text-ink-secondary">
                    {batch.successfulMoves} {messages.dashboard.filesMovedLabel} ·{' '}
                    {batch.failedMoves} {messages.dashboard.filesFailedLabel} ·{' '}
                    {formatBytes(batch.totalBytes)}
                  </p>
                </button>
              );
            })}
          </div>

          <section
            aria-label={messages.history.colMeta}
            className="flex min-h-[420px] flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface-elevated"
          >
            {loadingDetail ? (
              <div className="p-5">
                <SkeletonBlock rows={4} />
              </div>
            ) : !detail ? (
              <div className="flex flex-1 flex-col items-center justify-center p-8 text-center">
                <p className="text-sm font-medium text-ink">
                  {messages.history.detailPlaceholderTitle}
                </p>
                <p className="mt-1 text-xs text-ink-muted">
                  {messages.history.detailPlaceholderDesc}
                </p>
              </div>
            ) : (
              <>
                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle px-5 py-3.5">
                  <div className="min-w-0 space-y-1">
                    <div className="flex items-center gap-2.5">
                      <BatchStatusBadge status={detail.batch.status} />
                      <span className="font-mono text-[11px] text-ink-muted">
                        {detail.batch.id}
                      </span>
                    </div>
                    <PathDisplay path={detail.batch.sourceFolder} maxLength={64} />
                  </div>

                  {hasUndoableItems && (
                    <button
                      type="button"
                      onClick={() => setUndoTarget({ batchId: detail.batch.id, itemId: null })}
                      className="inline-flex items-center gap-1.5 rounded-md border border-border-strong bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
                    >
                      <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
                      <span>{messages.history.undoBatchButton}</span>
                    </button>
                  )}
                </div>

                {detail.items.length === 0 ? (
                  <div className="p-8 text-center text-xs text-ink-muted">
                    {messages.history.emptyMoves}
                  </div>
                ) : (
                  <>
                    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_168px] gap-3 border-b border-border-subtle bg-surface-inset px-5 py-2 text-[11px] font-medium text-ink-muted">
                      <span>{messages.history.colOriginal}</span>
                      <span>{messages.history.colDestination}</span>
                      <span className="text-right">{messages.history.colActions}</span>
                    </div>
                    <div ref={detailScrollRef} className="min-h-0 flex-1 overflow-y-auto">
                      <div
                        style={{
                          height: `${rowVirtualizer.getTotalSize()}px`,
                          position: 'relative',
                          width: '100%',
                        }}
                      >
                        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                          const item = items[virtualRow.index];
                          if (!item) {
                            return null;
                          }
                          return (
                            <div
                              key={item.id}
                              style={{
                                position: 'absolute',
                                top: 0,
                                left: 0,
                                width: '100%',
                                height: `${virtualRow.size}px`,
                                transform: `translateY(${virtualRow.start}px)`,
                              }}
                              className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_168px] items-center gap-3 border-b border-border-subtle px-5 text-xs hover:bg-surface-hover"
                            >
                              <div className="min-w-0 space-y-0.5">
                                <PathDisplay path={item.originalPath} maxLength={42} />
                                <p className="tabular-nums text-[11px] text-ink-muted">
                                  {formatBytes(item.fileSize)}
                                  {item.crossVolume
                                    ? ` · ${messages.history.crossVolumeBadge}`
                                    : ''}
                                </p>
                              </div>

                              <div className="min-w-0 space-y-0.5">
                                <PathDisplay path={item.newPath} maxLength={42} />
                                {item.undone && (
                                  <span className="inline-block rounded border border-neutral-border bg-neutral-bg px-1.5 py-0.5 text-[10px] font-medium text-neutral-ink">
                                    {messages.history.undoneBadge}
                                  </span>
                                )}
                                {!item.undone && item.undoNote && (
                                  <p
                                    className="truncate text-[11px] text-warning-ink"
                                    title={item.undoNote}
                                  >
                                    {item.undoNote}
                                  </p>
                                )}
                              </div>

                              <div className="flex items-center justify-end gap-1.5">
                                {!item.undone && detail.batch.kind !== 'undo' && (
                                  <button
                                    type="button"
                                    onClick={() =>
                                      setUndoTarget({ batchId: detail.batch.id, itemId: item.id })
                                    }
                                    className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface px-2 py-1 text-[11px] font-medium text-ink hover:bg-surface-hover"
                                  >
                                    <RotateCcw className="h-3 w-3" aria-hidden="true" />
                                    <span>{messages.history.undoItemButton}</span>
                                  </button>
                                )}
                                <button
                                  type="button"
                                  onClick={() => void handleRevealInExplorer(item.id)}
                                  aria-label={`${messages.history.revealInExplorer}: ${item.newPath}`}
                                  title={messages.history.revealInExplorer}
                                  className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface p-1.5 text-ink-muted hover:bg-surface-hover hover:text-ink"
                                >
                                  <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                                </button>
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  </>
                )}
              </>
            )}
          </section>
        </div>
      )}

      <ConfirmDialog
        open={undoTarget !== null}
        title={messages.history.undoDialogTitle}
        description={messages.history.undoDialogDesc}
        willHappen={[messages.history.undoWillHappen]}
        willNotHappen={[messages.history.undoWillNotHappen]}
        confirmLabel={
          undoTarget?.itemId !== null
            ? messages.history.undoItemButton
            : messages.history.undoBatchButton
        }
        busy={undoing}
        onConfirm={() => void handleConfirmUndo()}
        onCancel={() => setUndoTarget(null)}
      />
    </div>
  );
}
