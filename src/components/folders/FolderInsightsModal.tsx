import { useEffect, useId } from 'react';
import { CopyCheck, HardDrive, ShieldCheck, X } from 'lucide-react';
import { messages } from '../../i18n';
import type { FolderStorageReport } from '../../types/generated';
import { formatBytes, formatNumber, formatTimestamp } from '../../utils/format';
import { PathDisplay } from '../common/PathDisplay';

interface FolderInsightsModalProps {
  report: FolderStorageReport;
  onClose: () => void;
}

export function FolderInsightsModal({ report, onClose }: FolderInsightsModalProps) {
  const titleId = useId();

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  const totalDuplicateWaste = report.duplicateGroups.reduce((sum, g) => sum + g.wastedBytes, 0);
  const maxCategoryBytes = Math.max(1, ...report.categories.map((c) => c.totalBytes));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="flex max-h-[88vh] w-full max-w-4xl flex-col overflow-hidden rounded-lg border border-border-strong bg-surface-elevated shadow-xl"
      >
        <div className="flex items-start justify-between gap-4 border-b border-border-subtle px-5 py-3.5">
          <div className="min-w-0">
            <h2 id={titleId} className="text-sm font-semibold text-ink">
              {messages.insights.title}
            </h2>
            <p className="mt-0.5 text-xs text-ink-muted">{messages.insights.subtitle}</p>
            <div className="mt-1">
              <PathDisplay path={report.folderPath} maxLength={76} />
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            aria-label={messages.insights.closeAriaLabel}
            className="rounded p-1.5 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <div className="grid grid-cols-2 divide-y divide-border-subtle border-b border-border-subtle bg-surface sm:grid-cols-4 sm:divide-x sm:divide-y-0">
          <div className="px-4 py-3">
            <p className="text-[11px] text-ink-muted">{messages.insights.statFiles}</p>
            <p className="tabular-nums mt-1 text-lg font-semibold text-ink">
              {formatNumber(report.totalFiles)}
            </p>
          </div>
          <div className="px-4 py-3">
            <p className="text-[11px] text-ink-muted">{messages.insights.statTotalSize}</p>
            <p className="tabular-nums mt-1 text-lg font-semibold text-ink">
              {formatBytes(report.totalBytes)}
            </p>
          </div>
          <div className="px-4 py-3">
            <p className="text-[11px] text-ink-muted">{messages.insights.statOldFiles}</p>
            <p className="tabular-nums mt-1 text-lg font-semibold text-warning-ink">
              {formatNumber(report.oldFilesCount)} ({formatBytes(report.oldFilesBytes)})
            </p>
          </div>
          <div className="px-4 py-3">
            <p className="text-[11px] text-ink-muted">{messages.insights.statDuplicateWaste}</p>
            <p className="tabular-nums mt-1 text-lg font-semibold text-accent">
              {formatBytes(totalDuplicateWaste)}
            </p>
          </div>
        </div>

        <div className="min-h-0 flex-1 space-y-6 overflow-y-auto p-5">
          <section aria-labelledby="insights-categories-heading" className="space-y-3">
            <h3 id="insights-categories-heading" className="text-xs font-semibold text-ink">
              {messages.insights.categoriesTitle}
            </h3>
            {report.categories.length === 0 ? (
              <p className="text-xs text-ink-muted">{messages.insights.noLargeFiles}</p>
            ) : (
              <div className="space-y-2 rounded-lg border border-border-subtle bg-surface p-4">
                {report.categories.map((cat, idx) => {
                  const widthPct = Math.max(
                    2,
                    Math.round((cat.totalBytes / maxCategoryBytes) * 100),
                  );
                  return (
                    <div key={cat.name} className="space-y-1 text-xs">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-medium text-ink">{cat.name}</span>
                        <span className="tabular-nums text-[11px] text-ink-secondary">
                          {formatNumber(cat.fileCount)} files | {formatBytes(cat.totalBytes)}
                        </span>
                      </div>
                      <div className="h-2 w-full overflow-hidden rounded-sm bg-surface-inset">
                        <div
                          style={{ width: `${widthPct}%` }}
                          className={`h-full rounded-sm ${idx === 0 ? 'bg-accent' : 'bg-ink-muted'}`}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          <section aria-labelledby="insights-duplicates-heading" className="space-y-3">
            <div className="flex items-center gap-2">
              <CopyCheck className="h-4 w-4 text-accent" aria-hidden="true" />
              <h3 id="insights-duplicates-heading" className="text-xs font-semibold text-ink">
                {messages.insights.duplicatesTitle}
              </h3>
            </div>

            <div className="flex items-start gap-2 rounded-md border border-border-subtle bg-surface-inset px-3 py-2 text-[11px] text-ink-secondary">
              <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" />
              <span>{messages.insights.duplicatesSafetyNote}</span>
            </div>

            {report.duplicateGroups.length === 0 ? (
              <p className="text-xs text-ink-muted">{messages.insights.noDuplicates}</p>
            ) : (
              <div className="space-y-2">
                {report.duplicateGroups.map((grp) => (
                  <div
                    key={grp.sha256}
                    className="rounded-lg border border-border-subtle bg-surface p-3 text-xs"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle pb-2">
                      <span className="tabular-nums font-medium text-ink">
                        {grp.paths.length} copies ({formatBytes(grp.sizeBytes)}{' '}
                        {messages.insights.eachSizeLabel})
                      </span>
                      <span className="tabular-nums rounded border border-warning-border bg-warning-bg px-2 py-0.5 text-[11px] font-medium text-warning-ink">
                        {formatBytes(grp.wastedBytes)} {messages.insights.extraCopyWasteLabel}
                      </span>
                    </div>
                    <ul className="mt-2 space-y-1">
                      {grp.paths.map((p) => (
                        <li key={p}>
                          <PathDisplay path={p} maxLength={72} />
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            )}
          </section>

          <section aria-labelledby="insights-largest-heading" className="space-y-3">
            <div className="flex items-center gap-2">
              <HardDrive className="h-4 w-4 text-ink-muted" aria-hidden="true" />
              <h3 id="insights-largest-heading" className="text-xs font-semibold text-ink">
                {messages.insights.largestFilesTitle}
              </h3>
            </div>

            {report.largestFiles.length === 0 ? (
              <p className="text-xs text-ink-muted">{messages.insights.noLargeFiles}</p>
            ) : (
              <div className="divide-y divide-border-subtle overflow-hidden rounded-lg border border-border-subtle bg-surface">
                {report.largestFiles.map((file) => (
                  <div
                    key={file.path}
                    className="flex flex-wrap items-center justify-between gap-3 px-4 py-2.5 text-xs"
                  >
                    <div className="min-w-0 flex-1 space-y-0.5">
                      <PathDisplay path={file.path} maxLength={64} />
                      <p className="tabular-nums text-[11px] text-ink-muted">
                        {file.matchedRuleName ?? messages.common.none} |{' '}
                        {formatTimestamp(file.modifiedEpochMs)}
                      </p>
                    </div>
                    <span className="tabular-nums font-mono font-semibold text-ink">
                      {formatBytes(file.sizeBytes)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>

        <div className="flex justify-end border-t border-border-subtle bg-surface px-5 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink hover:bg-surface-hover"
          >
            {messages.common.close}
          </button>
        </div>
      </div>
    </div>
  );
}
