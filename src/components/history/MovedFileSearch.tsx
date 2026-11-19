import { useState, type FormEvent } from 'react';
import { ExternalLink, RotateCcw, Search, X } from 'lucide-react';
import { useAppData } from '../../hooks/useAppData';
import { useToast } from '../../hooks/useToast';
import { messages } from '../../i18n';
import * as ipc from '../../services/ipc';
import type { MoveLogItem } from '../../types/generated';
import { formatBytes, formatTimestamp } from '../../utils/format';
import { resolveErrorMessage } from '../../utils/paths';
import { PathDisplay } from '../common/PathDisplay';
import { ConfirmDialog } from '../dialogs/ConfirmDialog';

export function MovedFileSearch() {
  const { isDesktopConnected, refreshAll } = useAppData();
  const { pushToast } = useToast();

  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [results, setResults] = useState<MoveLogItem[]>([]);
  const [undoItem, setUndoItem] = useState<MoveLogItem | null>(null);
  const [undoing, setUndoing] = useState(false);

  const runSearch = async (rawQuery: string) => {
    const trimmed = rawQuery.trim();
    if (!trimmed) {
      setResults([]);
      setHasSearched(false);
      return;
    }
    if (!isDesktopConnected) {
      return;
    }

    setSearching(true);
    try {
      const hits = await ipc.searchMoveLogs(trimmed, 20);
      setResults(hits);
      setHasSearched(true);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setSearching(false);
    }
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    void runSearch(query);
  };

  const handleClear = () => {
    setQuery('');
    setResults([]);
    setHasSearched(false);
  };

  const handleReveal = async (moveLogId: number) => {
    try {
      await ipc.revealMoveLogInExplorer(moveLogId);
    } catch (err) {
      pushToast('error', `${messages.history.revealFailedPrefix} ${resolveErrorMessage(err)}`);
    }
  };

  const handleConfirmPutBack = async () => {
    if (!undoItem) {
      return;
    }
    setUndoing(true);
    try {
      const res = await ipc.undoBatch({
        batchId: undoItem.batchId,
        itemIds: [undoItem.id],
      });
      setUndoItem(null);
      if (res.status === 'completed') {
        pushToast('success', messages.history.undoCompletedToast);
      } else if (res.status === 'partial') {
        pushToast('warning', messages.history.undoPartialToast);
      } else {
        pushToast('error', messages.history.undoFailedToast);
      }
      await Promise.all([runSearch(query), refreshAll()]);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setUndoing(false);
    }
  };

  return (
    <section
      aria-labelledby="moved-file-search-heading"
      className="rounded-lg border border-border-subtle bg-surface-elevated p-4"
    >
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 id="moved-file-search-heading" className="text-sm font-semibold text-ink">
            {messages.fileSearch.title}
          </h2>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.fileSearch.subtitle}</p>
        </div>

        <form onSubmit={handleSubmit} className="flex w-full max-w-md items-center gap-2">
          <div className="relative flex-1">
            <Search
              className="pointer-events-none absolute top-2 left-2.5 h-3.5 w-3.5 text-ink-muted"
              aria-hidden="true"
            />
            <input
              type="search"
              aria-label={messages.fileSearch.inputLabel}
              placeholder={messages.fileSearch.placeholder}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className="w-full rounded-md border border-border-subtle bg-surface-inset py-1.5 pr-7 pl-8 text-xs text-ink placeholder:text-ink-muted"
            />
            {query.length > 0 && (
              <button
                type="button"
                onClick={handleClear}
                aria-label={messages.fileSearch.clearButton}
                className="absolute top-1.5 right-1.5 rounded p-0.5 text-ink-muted hover:text-ink"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            )}
          </div>
          <button
            type="submit"
            disabled={searching || query.trim().length === 0}
            className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-40"
          >
            {messages.fileSearch.searchButton}
          </button>
        </form>
      </div>

      {searching && <p className="mt-3 text-xs text-ink-muted">{messages.fileSearch.searching}</p>}

      {!searching && hasSearched && results.length === 0 && (
        <p className="mt-3 rounded border border-border-subtle bg-surface-inset px-3 py-2 text-xs text-ink-muted">
          {messages.fileSearch.noResults}
        </p>
      )}

      {!searching && results.length > 0 && (
        <div className="mt-3 divide-y divide-border-subtle overflow-hidden rounded-md border border-border-subtle bg-surface">
          {results.map((item) => (
            <div
              key={item.id}
              className="flex flex-wrap items-center justify-between gap-3 px-3.5 py-2.5 text-xs"
            >
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[11px] font-medium text-ink-muted">
                    {messages.fileSearch.movedToLabel}:
                  </span>
                  <PathDisplay path={item.newPath} maxLength={58} />
                </div>
                <div className="flex flex-wrap items-center gap-3 text-[11px] text-ink-muted">
                  <span>
                    {messages.fileSearch.movedFromLabel}:{' '}
                    <PathDisplay path={item.originalPath} maxLength={44} showCopy={false} />
                  </span>
                  <span className="tabular-nums">{formatBytes(item.fileSize)}</span>
                  <span className="tabular-nums">{formatTimestamp(item.movedAt)}</span>
                  {item.undone && (
                    <span className="rounded border border-neutral-border bg-neutral-bg px-1.5 py-0.5 text-[10px] font-medium text-neutral-ink">
                      {messages.fileSearch.restoredBadge}
                    </span>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-1.5">
                {!item.undone && (
                  <button
                    type="button"
                    onClick={() => setUndoItem(item)}
                    className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface-elevated px-2.5 py-1 text-[11px] font-medium text-ink hover:bg-surface-hover"
                  >
                    <RotateCcw className="h-3 w-3" aria-hidden="true" />
                    <span>{messages.fileSearch.putBackButton}</span>
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void handleReveal(item.id)}
                  className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface-elevated px-2.5 py-1 text-[11px] font-medium text-ink hover:bg-surface-hover"
                >
                  <ExternalLink className="h-3 w-3" aria-hidden="true" />
                  <span>{messages.fileSearch.showInExplorerButton}</span>
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <ConfirmDialog
        open={undoItem !== null}
        title={messages.history.undoDialogTitle}
        description={messages.history.undoDialogDesc}
        willHappen={[messages.history.undoWillHappen]}
        willNotHappen={[messages.history.undoWillNotHappen]}
        confirmLabel={messages.fileSearch.putBackButton}
        busy={undoing}
        onConfirm={() => void handleConfirmPutBack()}
        onCancel={() => setUndoItem(null)}
      />
    </section>
  );
}
