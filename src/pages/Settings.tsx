import { useId, useState } from 'react';
import { Download, FolderOpen } from 'lucide-react';
import { PathDisplay } from '../components/common/PathDisplay';
import { SegmentedControl } from '../components/common/SegmentedControl';
import { Switch } from '../components/common/Switch';
import { useAppData } from '../hooks/useAppData';
import { useToast } from '../hooks/useToast';
import { messages } from '../i18n';
import * as ipc from '../services/ipc';
import type { AppSettings, ThemeMode } from '../types/generated';
import { downloadTextFile, formatBytes, formatNumber } from '../utils/format';
import { resolveErrorMessage } from '../utils/paths';

const retentionOptions: readonly { label: string; value: number | null }[] = [
  { label: messages.settings.retention30, value: 30 },
  { label: messages.settings.retention90, value: 90 },
  { label: messages.settings.retention365, value: 365 },
  { label: messages.settings.retentionForever, value: null },
];

export function Settings() {
  const { appInfo, dbStatus, settings, saveSettings } = useAppData();
  const { pushToast } = useToast();

  const depthId = useId();
  const retentionId = useId();
  const [exporting, setExporting] = useState(false);

  const patchSettings = async (patch: Partial<AppSettings>) => {
    try {
      await saveSettings({ ...settings, ...patch });
      pushToast('success', messages.settings.savedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleOpenLogs = async () => {
    try {
      await ipc.openLogsFolder();
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  const handleExportDiagnostics = async () => {
    setExporting(true);
    try {
      const diag = await ipc.exportRedactedDiagnostics();
      downloadTextFile(
        'sortdeck-diagnostics-redacted.json',
        JSON.stringify(diag, null, 2),
        'application/json',
      );
      pushToast('info', messages.settings.diagnosticsExportedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="space-y-6">
      <header className="border-b border-border-subtle pb-4">
        <h1 className="text-lg font-semibold tracking-tight text-ink">{messages.settings.title}</h1>
        <p className="mt-0.5 text-xs text-ink-muted">{messages.settings.subtitle}</p>
      </header>

      <section
        aria-labelledby="settings-appearance"
        className="rounded-lg border border-border-subtle bg-surface-elevated p-5"
      >
        <h2 id="settings-appearance" className="text-sm font-semibold text-ink">
          {messages.settings.sectionAppearance}
        </h2>
        <div className="mt-4 flex flex-wrap items-center justify-between gap-4">
          <span className="text-xs font-medium text-ink-secondary">
            {messages.settings.themeLabel}
          </span>
          <SegmentedControl<ThemeMode>
            ariaLabel={messages.settings.themeLabel}
            value={settings.theme}
            onChange={(theme) => void patchSettings({ theme })}
            options={[
              { value: 'dark', label: messages.settings.themeDark },
              { value: 'light', label: messages.settings.themeLight },
              { value: 'system', label: messages.settings.themeSystem },
            ]}
          />
        </div>
      </section>

      <section
        aria-labelledby="settings-automation"
        className="rounded-lg border border-border-subtle bg-surface-elevated p-5"
      >
        <h2 id="settings-automation" className="text-sm font-semibold text-ink">
          {messages.settings.sectionAutomation}
        </h2>
        <div className="mt-4 divide-y divide-border-subtle">
          <div className="py-3 first:pt-0">
            <Switch
              checked={settings.globalAutoWatch}
              onChange={(globalAutoWatch) => void patchSettings({ globalAutoWatch })}
              label={messages.settings.globalAutoWatchLabel}
            />
          </div>
          <div className="py-3">
            <Switch
              checked={settings.launchOnStartup}
              onChange={(launchOnStartup) => void patchSettings({ launchOnStartup })}
              label={messages.settings.launchOnStartupLabel}
              description={messages.settings.launchOnStartupHint}
            />
          </div>
          <div className="py-3">
            <Switch
              checked={settings.startMinimized}
              onChange={(startMinimized) => void patchSettings({ startMinimized })}
              label={messages.settings.startMinimizedLabel}
            />
          </div>
          <div className="py-3">
            <Switch
              checked={settings.minimizeToTray}
              onChange={(minimizeToTray) => void patchSettings({ minimizeToTray })}
              label={messages.settings.minimizeToTrayLabel}
            />
          </div>
          <div className="py-3 last:pb-0">
            <Switch
              checked={settings.notifications}
              onChange={(notifications) => void patchSettings({ notifications })}
              label={messages.settings.notificationsLabel}
            />
          </div>
        </div>
      </section>

      <section
        aria-labelledby="settings-scanning"
        className="rounded-lg border border-border-subtle bg-surface-elevated p-5"
      >
        <h2 id="settings-scanning" className="text-sm font-semibold text-ink">
          {messages.settings.sectionScanning}
        </h2>
        <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor={depthId} className="block text-xs font-medium text-ink">
              {messages.settings.defaultDepthLabel}
            </label>
            <input
              id={depthId}
              type="number"
              min={1}
              max={32}
              value={settings.defaultScanDepth}
              onChange={(e) => {
                const parsed = Number.parseInt(e.target.value, 10);
                if (!Number.isNaN(parsed) && parsed >= 1 && parsed <= 32) {
                  void patchSettings({ defaultScanDepth: parsed });
                }
              }}
              className="tabular-nums mt-1.5 w-full rounded-md border border-border-subtle bg-surface-inset px-3 py-2 text-xs text-ink"
            />
          </div>

          <div>
            <label htmlFor={retentionId} className="block text-xs font-medium text-ink">
              {messages.settings.retentionLabel}
            </label>
            <select
              id={retentionId}
              value={settings.retentionDays === null ? 'forever' : String(settings.retentionDays)}
              onChange={(e) => {
                const selected = retentionOptions.find(
                  (opt) => (opt.value === null ? 'forever' : String(opt.value)) === e.target.value,
                );
                if (selected) {
                  void patchSettings({ retentionDays: selected.value });
                }
              }}
              className="mt-1.5 w-full rounded-md border border-border-subtle bg-surface-inset px-3 py-2 text-xs text-ink"
            >
              {retentionOptions.map((opt) => (
                <option
                  key={opt.value === null ? 'forever' : opt.value}
                  value={opt.value === null ? 'forever' : String(opt.value)}
                >
                  {opt.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      </section>

      <section
        aria-labelledby="settings-diagnostics"
        className="rounded-lg border border-border-subtle bg-surface-elevated p-5"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 id="settings-diagnostics" className="text-sm font-semibold text-ink">
            {messages.settings.sectionDiagnostics}
          </h2>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void handleOpenLogs()}
              className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover"
            >
              <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{messages.settings.openLogsButton}</span>
            </button>
            <button
              type="button"
              disabled={exporting}
              onClick={() => void handleExportDiagnostics()}
              className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
            >
              <Download className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{messages.settings.exportDiagnosticsButton}</span>
            </button>
          </div>
        </div>

        <dl className="mt-4 grid grid-cols-1 gap-3 border-t border-border-subtle pt-4 text-xs sm:grid-cols-2 lg:grid-cols-3">
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.versionLabel}</dt>
            <dd className="tabular-nums mt-0.5 font-mono text-ink">
              {appInfo?.version ?? '0.1.0'}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.identifierLabel}</dt>
            <dd className="mt-0.5 font-mono text-ink">
              {appInfo?.identifier ?? 'com.sortdeck.app'}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.schemaLabel}</dt>
            <dd className="tabular-nums mt-0.5 font-mono text-ink">
              v{dbStatus?.schemaVersion ?? appInfo?.schemaVersion ?? 1}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.revisionLabel}</dt>
            <dd className="tabular-nums mt-0.5 font-mono text-ink">
              {dbStatus ? formatNumber(dbStatus.configRevision) : '-'}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.dbSizeLabel}</dt>
            <dd className="tabular-nums mt-0.5 font-mono text-ink">
              {dbStatus ? formatBytes(dbStatus.dbSizeBytes) : '-'}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] text-ink-muted">{messages.settings.dbPathLabel}</dt>
            <dd className="mt-0.5">
              {dbStatus?.dbPath ? (
                <PathDisplay path={dbStatus.dbPath} maxLength={40} />
              ) : (
                <span className="font-mono text-ink-muted">-</span>
              )}
            </dd>
          </div>
        </dl>

        {dbStatus?.recoveredFromCorruption && (
          <div
            role="status"
            className="mt-4 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-ink"
          >
            <p className="font-medium">{messages.settings.corruptionRecoveredTitle}</p>
            {dbStatus.corruptBackupPath && (
              <p className="mt-1 font-mono text-[11px]">
                {messages.settings.corruptionBackupLabel} {dbStatus.corruptBackupPath}
              </p>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
