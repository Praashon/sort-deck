import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { ArrowRight, FolderOpen, History, Pause, Play } from 'lucide-react';
import { Link } from 'react-router';
import { EmptyState } from '../components/common/EmptyState';
import { PathDisplay } from '../components/common/PathDisplay';
import { SkeletonBlock } from '../components/common/SkeletonBlock';
import { BatchStatusBadge, FolderStatusBadge } from '../components/common/StatusBadge';
import { DryRunPreviewModal } from '../components/folders/DryRunPreviewModal';
import { FirstRunBanner } from '../components/folders/FirstRunBanner';
import { MovedFileSearch } from '../components/history/MovedFileSearch';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type { PlanSummary } from '../types/generated';
import { formatBytes, formatNumber, formatTimestamp } from '../utils/format';
import { resolveErrorMessage } from '../utils/paths';

export function Dashboard() {
  const {
    isDesktopConnected,
    loading,
    settings,
    folders,
    rulesData,
    recentBatches,
    suggestedDownloads,
    firstRunDismissed,
    dismissFirstRun,
    saveSettings,
    refreshAll,
  } = useAppData();
  const { pushToast } = useToast();

  const [busyFolderAction, setBusyFolderAction] = useState(false);
  const [planningFolderId, setPlanningFolderId] = useState<number | null>(null);
  const [activePlan, setActivePlan] = useState<PlanSummary | null>(null);

  const enabledFolders = folders.filter((f) => f.enabled);
  const enabledRulesCount = rulesData.rules.filter((r) => r.enabled).length;
  const totalRecentMoves = recentBatches.reduce((sum, b) => sum + b.successfulMoves, 0);
  const totalRecentBytes = recentBatches.reduce((sum, b) => sum + b.totalBytes, 0);

  const handlePreviewFolder = async (folderId: number) => {
    setPlanningFolderId(folderId);
    try {
      const plan = await ipc.createDryRunPlan(folderId);
      setActivePlan(plan);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setPlanningFolderId(null);
    }
  };

  const handleAddFolderPath = async (selectedPath: string) => {
    setBusyFolderAction(true);
    try {
      await ipc.addWatchedFolder({
        path: selectedPath,
        recursive: false,
        maxDepth: settings.defaultScanDepth,
        minFileAgeSecs: 5,
        allowNetworkRoot: false,
      });
      await refreshAll();
      pushToast('success', messages.folders.addedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setBusyFolderAction(false);
    }
  };

  const handlePickFolder = async () => {
    if (!isDesktopConnected) {
      pushToast('info', messages.folders.pickerUnavailable);
      return;
    }
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === 'string') {
        await handleAddFolderPath(selected);
      }
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleToggleAutoWatch = async () => {
    try {
      await saveSettings({ ...settings, globalAutoWatch: !settings.globalAutoWatch });
      pushToast('info', messages.settings.savedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const firstEnabledFolder = enabledFolders[0];

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle pb-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-ink">
            {messages.dashboard.title}
          </h1>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.dashboard.subtitle}</p>
        </div>

        {firstEnabledFolder && (
          <button
            type="button"
            disabled={planningFolderId !== null}
            onClick={() => void handlePreviewFolder(firstEnabledFolder.id)}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
          >
            <Play className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.dashboard.quickSweepAction}</span>
          </button>
        )}
      </header>

      {loading ? (
        <SkeletonBlock rows={4} />
      ) : (
        <>
          {folders.length === 0 && !firstRunDismissed && (
            <FirstRunBanner
              suggestedDownloads={suggestedDownloads}
              busy={busyFolderAction}
              onAddDownloads={(path) => void handleAddFolderPath(path)}
              onChooseFolder={() => void handlePickFolder()}
              onDismiss={dismissFirstRun}
            />
          )}

          <section
            aria-label="Workspace metrics"
            className="grid grid-cols-2 divide-y divide-border-subtle overflow-hidden rounded-lg border border-border-subtle bg-surface-elevated sm:grid-cols-4 sm:divide-x sm:divide-y-0"
          >
            <div className="p-4">
              <p className="text-xs text-ink-muted">{messages.dashboard.kpiFolders}</p>
              <p className="tabular-nums mt-1.5 text-2xl font-semibold tracking-tight text-ink">
                {formatNumber(folders.length)}
              </p>
            </div>
            <div className="p-4">
              <p className="text-xs text-ink-muted">{messages.dashboard.kpiRules}</p>
              <p className="tabular-nums mt-1.5 text-2xl font-semibold tracking-tight text-ink">
                {formatNumber(enabledRulesCount)}
              </p>
            </div>
            <div className="p-4">
              <p className="text-xs text-ink-muted">{messages.dashboard.kpiRecentMoves}</p>
              <p className="tabular-nums mt-1.5 text-2xl font-semibold tracking-tight text-ink">
                {formatNumber(totalRecentMoves)}
              </p>
            </div>
            <div className="p-4">
              <p className="text-xs text-ink-muted">{messages.dashboard.kpiRecentBytes}</p>
              <p className="tabular-nums mt-1.5 text-2xl font-semibold tracking-tight text-ink">
                {formatBytes(totalRecentBytes)}
              </p>
            </div>
          </section>

          <section aria-labelledby="autowatch-heading" className="space-y-4">
            <MovedFileSearch />
            <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-border-subtle bg-surface-elevated px-5 py-4">
              <div>
                <h2 id="autowatch-heading" className="text-sm font-medium text-ink">
                  {messages.dashboard.autoWatchCardTitle}
                </h2>
                <p className="mt-0.5 text-xs text-ink-muted">
                  {settings.globalAutoWatch
                    ? messages.dashboard.autoWatchCardActiveDesc
                    : messages.dashboard.autoWatchCardPausedDesc}
                </p>
              </div>
              <button
                type="button"
                onClick={() => void handleToggleAutoWatch()}
                className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
              >
                {settings.globalAutoWatch ? (
                  <>
                    <Pause className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{messages.nav.pauseAutoWatch}</span>
                  </>
                ) : (
                  <>
                    <Play className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{messages.nav.resumeAutoWatch}</span>
                  </>
                )}
              </button>
            </div>
          </section>

          <section aria-labelledby="quick-folders-heading" className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 id="quick-folders-heading" className="text-sm font-semibold text-ink">
                {messages.dashboard.quickFoldersTitle}
              </h2>
              <Link
                to="/folders"
                className="inline-flex items-center gap-1 text-xs font-medium text-accent hover:underline"
              >
                <span>{messages.nav.folders}</span>
                <ArrowRight className="h-3 w-3" aria-hidden="true" />
              </Link>
            </div>

            {enabledFolders.length === 0 ? (
              <EmptyState
                icon={FolderOpen}
                headingLevel="h3"
                title={messages.dashboard.quickFoldersEmptyTitle}
                description={messages.dashboard.quickFoldersEmptyDesc}
                action={
                  <Link
                    to="/folders"
                    className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink hover:bg-surface-hover"
                  >
                    <span>{messages.dashboard.quickFoldersEmptyAction}</span>
                  </Link>
                }
              />
            ) : (
              <div className="divide-y divide-border-subtle overflow-hidden rounded-lg border border-border-subtle bg-surface-elevated">
                {enabledFolders.map((folder) => (
                  <div
                    key={folder.id}
                    className="flex flex-wrap items-center justify-between gap-3 px-4 py-3"
                  >
                    <div className="min-w-0 flex-1 space-y-1">
                      <PathDisplay path={folder.path} maxLength={72} />
                      <div className="flex items-center gap-2">
                        <FolderStatusBadge status={folder.status} />
                        <span className="text-[11px] text-ink-muted">
                          {messages.folders.autoSortLabel}:{' '}
                          {folder.autoSort ? messages.common.yes : messages.common.no}
                        </span>
                      </div>
                    </div>
                    <button
                      type="button"
                      disabled={planningFolderId === folder.id}
                      onClick={() => void handlePreviewFolder(folder.id)}
                      className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
                    >
                      <Play className="h-3.5 w-3.5" aria-hidden="true" />
                      <span>
                        {planningFolderId === folder.id
                          ? messages.folders.planning
                          : messages.dashboard.previewButton}
                      </span>
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>

          <section aria-labelledby="recent-activity-heading" className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 id="recent-activity-heading" className="text-sm font-semibold text-ink">
                {messages.dashboard.recentActivityTitle}
              </h2>
              <Link
                to="/history"
                className="inline-flex items-center gap-1 text-xs font-medium text-accent hover:underline"
              >
                <span>{messages.dashboard.viewAllHistory}</span>
                <ArrowRight className="h-3 w-3" aria-hidden="true" />
              </Link>
            </div>

            {recentBatches.length === 0 ? (
              <EmptyState
                icon={History}
                headingLevel="h3"
                title={messages.dashboard.recentActivityEmptyTitle}
                description={messages.dashboard.recentActivityEmptyDesc}
              />
            ) : (
              <div className="overflow-hidden rounded-lg border border-border-subtle bg-surface-elevated">
                <div className="divide-y divide-border-subtle">
                  {recentBatches.slice(0, 5).map((batch) => (
                    <div
                      key={batch.id}
                      className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 text-xs"
                    >
                      <div className="flex min-w-0 items-center gap-3">
                        <BatchStatusBadge status={batch.status} />
                        <div className="min-w-0">
                          <p className="font-medium text-ink">
                            {messages.statuses.batchKind[batch.kind]}
                          </p>
                          <PathDisplay path={batch.sourceFolder} maxLength={54} showCopy={false} />
                        </div>
                      </div>
                      <div className="flex items-center gap-4">
                        <span className="tabular-nums text-ink-secondary">
                          {batch.successfulMoves} {messages.dashboard.filesMovedLabel} ·{' '}
                          {formatBytes(batch.totalBytes)}
                        </span>
                        <span className="tabular-nums text-ink-muted">
                          {formatTimestamp(batch.createdAt)}
                        </span>
                        <Link
                          to="/history"
                          className="rounded border border-border-subtle bg-surface px-2.5 py-1 text-[11px] font-medium text-ink hover:bg-surface-hover"
                        >
                          {messages.dashboard.inspectBatch}
                        </Link>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </section>
        </>
      )}

      {activePlan && (
        <DryRunPreviewModal
          plan={activePlan}
          onClose={() => setActivePlan(null)}
          onExecuted={async () => {
            await refreshAll();
          }}
        />
      )}
    </div>
  );
}
