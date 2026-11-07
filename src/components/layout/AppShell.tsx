import type { ReactNode } from 'react';
import { Link, NavLink } from 'react-router';
import {
  AlertTriangle,
  FolderOpen,
  History,
  LayoutDashboard,
  ListFilter,
  Pause,
  Play,
  Settings,
  ShieldAlert,
} from 'lucide-react';
import { useAppData } from '../../hooks/useAppData';
import { useToast } from '../../hooks/useToast';
import { messages } from '../../i18n';
import { resolveErrorMessage } from '../../utils/paths';
import { BrandLogo } from '../common/BrandLogo';

const navItems = [
  { to: '/', label: messages.nav.dashboard, Icon: LayoutDashboard },
  { to: '/folders', label: messages.nav.folders, Icon: FolderOpen },
  { to: '/rules', label: messages.nav.rules, Icon: ListFilter },
  { to: '/history', label: messages.nav.history, Icon: History },
  { to: '/recovery', label: messages.nav.recovery, Icon: ShieldAlert },
  { to: '/settings', label: messages.nav.settings, Icon: Settings },
] as const;

export function AppShell({ children }: { children: ReactNode }) {
  const {
    isDesktopConnected,
    appInfo,
    settings,
    folders,
    recoveryItems,
    saveSettings,
    error,
    refreshAll,
  } = useAppData();
  const { pushToast } = useToast();

  const unavailableCount = folders.filter((f) => f.status === 'unavailable').length;
  const recoveryCount = recoveryItems.length;

  const handleToggleGlobalAutoWatch = async () => {
    try {
      await saveSettings({
        ...settings,
        globalAutoWatch: !settings.globalAutoWatch,
      });
      pushToast('info', messages.settings.savedToast);
    } catch (err) {
      pushToast('error', resolveErrorMessage(err));
    }
  };

  return (
    <div className="flex h-screen w-full overflow-hidden bg-surface text-ink">
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-primary focus:px-3 focus:py-1.5 focus:text-xs focus:font-medium focus:text-primary-ink"
      >
        {messages.app.skipToContent}
      </a>

      <aside className="flex w-56 shrink-0 flex-col border-r border-border-subtle bg-canvas">
        <div className="flex h-14 items-center gap-2.5 border-b border-border-subtle px-4">
          <BrandLogo size={28} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="truncate text-sm font-semibold tracking-tight text-ink">
                {messages.app.name}
              </span>
              <span className="tabular-nums text-[11px] text-ink-muted">
                v{appInfo?.version ?? '0.1.0'}
              </span>
            </div>
            {!isDesktopConnected && (
              <span className="block text-[10px] font-medium text-ink-muted">
                {messages.app.previewModeBadge}
              </span>
            )}
          </div>
        </div>

        <nav aria-label={messages.nav.ariaLabel} className="flex-1 space-y-0.5 px-2 py-3">
          {navItems.map(({ to, label, Icon }) => {
            const showRecoveryBadge = to === '/recovery' && recoveryCount > 0;
            return (
              <NavLink
                key={to}
                to={to}
                end={to === '/'}
                className={({ isActive }) =>
                  `relative flex items-center justify-between rounded-md px-3 py-2 text-xs font-medium transition-colors ${
                    isActive
                      ? 'bg-surface-elevated text-ink'
                      : 'text-ink-muted hover:bg-surface-hover hover:text-ink'
                  }`
                }
              >
                {({ isActive }) => (
                  <>
                    {isActive && (
                      <span
                        aria-hidden="true"
                        className="absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-r bg-accent"
                      />
                    )}
                    <span className="flex items-center gap-2.5">
                      <Icon
                        className={`h-4 w-4 shrink-0 ${isActive ? 'text-accent' : ''}`}
                        aria-hidden="true"
                      />
                      <span>{label}</span>
                    </span>
                    {showRecoveryBadge && (
                      <span className="tabular-nums rounded border border-attention-border bg-attention-bg px-1.5 py-0.5 text-[10px] font-semibold text-attention-ink">
                        {recoveryCount}
                      </span>
                    )}
                  </>
                )}
              </NavLink>
            );
          })}
        </nav>

        <div className="border-t border-border-subtle p-3">
          <div className="rounded-md border border-border-subtle bg-surface-elevated p-2.5">
            <div className="flex items-center justify-between gap-2">
              <div className="min-w-0">
                <p className="text-[11px] font-medium text-ink-muted">
                  {messages.nav.autoWatchLabel}
                </p>
                <div className="mt-0.5 flex items-center gap-1.5">
                  <span
                    aria-hidden="true"
                    className={`h-1.5 w-1.5 rounded-full ${
                      settings.globalAutoWatch ? 'bg-accent' : 'bg-ink-muted'
                    }`}
                  />
                  <span className="text-xs font-medium text-ink">
                    {settings.globalAutoWatch
                      ? messages.nav.autoWatchActive
                      : messages.nav.autoWatchPaused}
                  </span>
                </div>
              </div>
              <button
                type="button"
                onClick={() => void handleToggleGlobalAutoWatch()}
                aria-label={
                  settings.globalAutoWatch
                    ? messages.nav.pauseAutoWatch
                    : messages.nav.resumeAutoWatch
                }
                className="inline-flex items-center gap-1 rounded border border-border-subtle bg-surface px-2 py-1 text-[11px] font-medium text-ink transition-colors hover:bg-surface-hover"
              >
                {settings.globalAutoWatch ? (
                  <>
                    <Pause className="h-3 w-3" aria-hidden="true" />
                    <span>{messages.nav.pauseAutoWatch}</span>
                  </>
                ) : (
                  <>
                    <Play className="h-3 w-3" aria-hidden="true" />
                    <span>{messages.nav.resumeAutoWatch}</span>
                  </>
                )}
              </button>
            </div>
            <p className="mt-1.5 text-[10px] leading-snug text-ink-muted">
              {messages.nav.phase5Hint}
            </p>
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {(recoveryCount > 0 || unavailableCount > 0 || error !== null || !isDesktopConnected) && (
          <div className="shrink-0 space-y-px border-b border-border-subtle bg-border-subtle">
            {recoveryCount > 0 && (
              <div
                role="alert"
                className="flex items-center justify-between gap-3 bg-attention-bg px-6 py-2 text-xs text-attention-ink"
              >
                <div className="flex items-center gap-2">
                  <ShieldAlert className="h-4 w-4 shrink-0" aria-hidden="true" />
                  <span className="font-medium">
                    {messages.alerts.recoveryBannerTitle} ({recoveryCount})
                  </span>
                </div>
                <Link
                  to="/recovery"
                  className="shrink-0 rounded border border-attention-border bg-surface px-2.5 py-1 text-[11px] font-medium text-ink transition-colors hover:bg-surface-hover"
                >
                  {messages.alerts.recoveryBannerAction}
                </Link>
              </div>
            )}

            {unavailableCount > 0 && (
              <div
                role="status"
                className="flex items-center justify-between gap-3 bg-warning-bg px-6 py-2 text-xs text-warning-ink"
              >
                <div className="flex items-center gap-2">
                  <AlertTriangle className="h-4 w-4 shrink-0" aria-hidden="true" />
                  <span className="font-medium">
                    {messages.alerts.unavailableFolderBannerTitle} ({unavailableCount})
                  </span>
                </div>
                <Link
                  to="/folders"
                  className="shrink-0 rounded border border-warning-border bg-surface px-2.5 py-1 text-[11px] font-medium text-ink transition-colors hover:bg-surface-hover"
                >
                  {messages.alerts.unavailableFolderBannerAction}
                </Link>
              </div>
            )}

            {error !== null && (
              <div
                role="alert"
                className="flex items-center justify-between gap-3 bg-danger-bg px-6 py-2 text-xs text-danger-ink"
              >
                <span>
                  {messages.app.loadErrorTitle}: {error}
                </span>
                <button
                  type="button"
                  onClick={() => void refreshAll()}
                  className="rounded border border-danger-border bg-surface px-2.5 py-1 text-[11px] font-medium text-ink hover:bg-surface-hover"
                >
                  {messages.app.retry}
                </button>
              </div>
            )}

            {!isDesktopConnected && (
              <div className="flex items-center justify-between gap-3 bg-surface-inset px-6 py-1.5 text-[11px] text-ink-muted">
                <span>{messages.app.previewModeNotice}</span>
              </div>
            )}
          </div>
        )}

        <main id="main-content" className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
          <div className="mx-auto max-w-6xl">{children}</div>
        </main>
      </div>
    </div>
  );
}
