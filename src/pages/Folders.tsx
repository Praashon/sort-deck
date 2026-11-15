import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen, FolderPlus, HardDrive, Play, Trash2 } from 'lucide-react';
import { EmptyState } from '../components/common/EmptyState';
import { PathDisplay } from '../components/common/PathDisplay';
import { SkeletonBlock } from '../components/common/SkeletonBlock';
import { FolderStatusBadge } from '../components/common/StatusBadge';
import { Switch } from '../components/common/Switch';
import { ConfirmDialog } from '../components/dialogs/ConfirmDialog';
import { DryRunPreviewModal } from '../components/folders/DryRunPreviewModal';
import { FolderInsightsModal } from '../components/folders/FolderInsightsModal';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type { FolderStorageReport, PlanSummary, WatchedFolder } from '../types/generated';
import { resolveErrorMessage } from '../utils/paths';

export function Folders() {
  const { isDesktopConnected, loading, settings, folders, setFolders, refreshAll } = useAppData();
  const { pushToast } = useToast();

  const [adding, setAdding] = useState(false);
  const [planningId, setPlanningId] = useState<number | null>(null);
  const [activePlan, setActivePlan] = useState<PlanSummary | null>(null);
  const [analyzingId, setAnalyzingId] = useState<number | null>(null);
  const [activeReport, setActiveReport] = useState<FolderStorageReport | null>(null);
  const [removeTarget, setRemoveTarget] = useState<WatchedFolder | null>(null);
  const [removing, setRemoving] = useState(false);

  const handleAddFolder = async () => {
    if (!isDesktopConnected) {
      pushToast('info', messages.folders.pickerUnavailable);
      return;
    }
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== 'string') {
        return;
      }
      setAdding(true);
      const added = await ipc.addWatchedFolder({
        path: selected,
        recursive: false,
        maxDepth: settings.defaultScanDepth,
        minFileAgeSecs: 5,
        allowNetworkRoot: false,
      });
      setFolders((prev) => [...prev, added]);
      pushToast('success', messages.folders.addedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setAdding(false);
    }
  };

  const handleUpdateFolder = async (folder: WatchedFolder, patch: Partial<WatchedFolder>) => {
    const next: WatchedFolder = { ...folder, ...patch };
    try {
      const saved = await ipc.updateWatchedFolder({
        id: next.id,
        enabled: next.enabled,
        autoSort: next.autoSort,
        recursive: next.recursive,
        maxDepth: Math.min(32, Math.max(1, next.maxDepth)),
        minFileAgeSecs: Math.max(0, next.minFileAgeSecs),
      });
      setFolders((prev) => prev.map((f) => (f.id === saved.id ? saved : f)));
      pushToast('success', messages.folders.updatedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handlePreviewSort = async (folderId: number) => {
    setPlanningId(folderId);
    try {
      const plan = await ipc.createDryRunPlan(folderId);
      setActivePlan(plan);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setPlanningId(null);
    }
  };

  const handleAnalyzeStorage = async (folderId: number) => {
    setAnalyzingId(folderId);
    try {
      const report = await ipc.analyzeFolderStorage(folderId);
      setActiveReport(report);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setAnalyzingId(null);
    }
  };

  const handleConfirmRemove = async () => {
    if (!removeTarget) {
      return;
    }
    setRemoving(true);
    try {
      await ipc.removeWatchedFolder(removeTarget.id);
      setFolders((prev) => prev.filter((f) => f.id !== removeTarget.id));
      setRemoveTarget(null);
      pushToast('info', messages.folders.removedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle pb-4">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-ink">
            {messages.folders.title}
          </h1>
          <p className="mt-0.5 text-xs text-ink-muted">{messages.folders.subtitle}</p>
        </div>

        <button
          type="button"
          disabled={adding}
          onClick={() => void handleAddFolder()}
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
        >
          <FolderPlus className="h-3.5 w-3.5" aria-hidden="true" />
          <span>{adding ? messages.folders.adding : messages.folders.addButton}</span>
        </button>
      </header>

      {loading ? (
        <SkeletonBlock rows={3} />
      ) : folders.length === 0 ? (
        <EmptyState
          icon={FolderOpen}
          title={messages.folders.emptyTitle}
          description={messages.folders.emptyDesc}
          action={
            <button
              type="button"
              disabled={adding}
              onClick={() => void handleAddFolder()}
              className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink hover:bg-primary-hover"
            >
              <FolderPlus className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{messages.folders.addButton}</span>
            </button>
          }
        />
      ) : (
        <div className="space-y-3">
          {folders.map((folder) => (
            <section
              key={folder.id}
              aria-label={folder.path}
              className="rounded-lg border border-border-subtle bg-surface-elevated"
            >
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle px-5 py-3.5">
                <div className="min-w-0 flex-1 space-y-1">
                  <PathDisplay path={folder.path} maxLength={76} />
                  <div>
                    <FolderStatusBadge status={folder.status} />
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    disabled={analyzingId === folder.id}
                    onClick={() => void handleAnalyzeStorage(folder.id)}
                    aria-label={`${messages.insights.checkSpaceButton}: ${folder.path}`}
                    className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
                  >
                    <HardDrive className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>
                      {analyzingId === folder.id
                        ? messages.insights.scanning
                        : messages.insights.checkSpaceButton}
                    </span>
                  </button>

                  <button
                    type="button"
                    disabled={!folder.enabled || planningId === folder.id}
                    onClick={() => void handlePreviewSort(folder.id)}
                    className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
                  >
                    <Play className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>
                      {planningId === folder.id
                        ? messages.folders.planning
                        : messages.folders.previewButton}
                    </span>
                  </button>

                  <button
                    type="button"
                    onClick={() => setRemoveTarget(folder)}
                    aria-label={`${messages.folders.removeButton}: ${folder.path}`}
                    className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-2.5 py-1.5 text-xs font-medium text-ink-secondary transition-colors hover:border-danger-border hover:bg-danger-bg hover:text-danger-ink"
                  >
                    <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{messages.folders.removeButton}</span>
                  </button>
                </div>
              </div>

              <div className="grid grid-cols-1 gap-4 px-5 py-4 md:grid-cols-2 lg:grid-cols-4">
                <Switch
                  checked={folder.enabled}
                  onChange={(enabled) => void handleUpdateFolder(folder, { enabled })}
                  label={messages.folders.enabledLabel}
                />

                <Switch
                  checked={folder.autoSort}
                  onChange={(autoSort) => void handleUpdateFolder(folder, { autoSort })}
                  label={messages.folders.autoSortLabel}
                  description={messages.folders.autoSortHint}
                />

                <Switch
                  checked={folder.recursive}
                  onChange={(recursive) => void handleUpdateFolder(folder, { recursive })}
                  label={messages.folders.recursiveLabel}
                />

                <div className="flex items-center gap-3">
                  <div className="flex-1">
                    <label
                      htmlFor={`depth-${folder.id}`}
                      className="block text-[11px] font-medium text-ink-muted"
                    >
                      {messages.folders.maxDepthLabel}
                    </label>
                    <input
                      id={`depth-${folder.id}`}
                      type="number"
                      min={1}
                      max={32}
                      disabled={!folder.recursive}
                      value={folder.maxDepth}
                      onChange={(e) => {
                        const parsed = Number.parseInt(e.target.value, 10);
                        if (!Number.isNaN(parsed)) {
                          void handleUpdateFolder(folder, { maxDepth: parsed });
                        }
                      }}
                      className="tabular-nums mt-1 w-full rounded border border-border-subtle bg-surface-inset px-2.5 py-1 text-xs text-ink disabled:opacity-40"
                    />
                  </div>

                  <div className="flex-1">
                    <label
                      htmlFor={`age-${folder.id}`}
                      className="block text-[11px] font-medium text-ink-muted"
                    >
                      {messages.folders.minAgeLabel}
                    </label>
                    <input
                      id={`age-${folder.id}`}
                      type="number"
                      min={0}
                      max={86400}
                      value={folder.minFileAgeSecs}
                      onChange={(e) => {
                        const parsed = Number.parseInt(e.target.value, 10);
                        if (!Number.isNaN(parsed)) {
                          void handleUpdateFolder(folder, { minFileAgeSecs: parsed });
                        }
                      }}
                      className="tabular-nums mt-1 w-full rounded border border-border-subtle bg-surface-inset px-2.5 py-1 text-xs text-ink"
                    />
                  </div>
                </div>
              </div>
            </section>
          ))}
        </div>
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

      {activeReport && (
        <FolderInsightsModal report={activeReport} onClose={() => setActiveReport(null)} />
      )}

      <ConfirmDialog
        open={removeTarget !== null}
        title={messages.folders.removeDialogTitle}
        description={messages.folders.removeDialogDesc}
        willHappen={[messages.folders.removeWillHappen]}
        willNotHappen={[messages.folders.removeWillNotHappen]}
        confirmLabel={messages.folders.removeButton}
        destructive
        busy={removing}
        onConfirm={() => void handleConfirmRemove()}
        onCancel={() => setRemoveTarget(null)}
      />
    </div>
  );
}
