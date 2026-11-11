import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import * as ipc from '../services/ipc';
import type {
  AppInfo,
  AppSettings,
  BatchSummary,
  DatabaseStatusInfo,
  RecoveryEntry,
  RulesListResponse,
  WatchedFolder,
} from '../types/generated';
import { resolveErrorMessage } from '../utils/paths';
import { useTheme } from './useTheme';

const DEFAULT_SETTINGS: AppSettings = {
  theme: 'dark',
  launchOnStartup: false,
  startMinimized: false,
  minimizeToTray: true,
  globalAutoWatch: true,
  defaultScanDepth: 1,
  retentionDays: 365,
  notifications: true,
};

const EMPTY_RULES: RulesListResponse = {
  rules: [],
  overlaps: [],
};

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

interface AppDataContextValue {
  isDesktopConnected: boolean;
  loading: boolean;
  error: string | null;
  appInfo: AppInfo | null;
  dbStatus: DatabaseStatusInfo | null;
  settings: AppSettings;
  folders: WatchedFolder[];
  rulesData: RulesListResponse;
  recoveryItems: RecoveryEntry[];
  recentBatches: BatchSummary[];
  suggestedDownloads: string | null;
  firstRunDismissed: boolean;
  dismissFirstRun: () => void;
  refreshAll: () => Promise<void>;
  saveSettings: (next: AppSettings) => Promise<AppSettings>;
  setFolders: React.Dispatch<React.SetStateAction<WatchedFolder[]>>;
  setRulesData: React.Dispatch<React.SetStateAction<RulesListResponse>>;
  setRecoveryItems: React.Dispatch<React.SetStateAction<RecoveryEntry[]>>;
}

const AppDataContext = createContext<AppDataContextValue | null>(null);

export function AppDataProvider({
  children,
  forceIpc,
}: {
  children: ReactNode;
  forceIpc?: boolean | undefined;
}) {
  const connected = forceIpc ?? isTauriRuntime();
  const [loading, setLoading] = useState<boolean>(connected);
  const [error, setError] = useState<string | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [dbStatus, setDbStatus] = useState<DatabaseStatusInfo | null>(null);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [folders, setFolders] = useState<WatchedFolder[]>([]);
  const [rulesData, setRulesData] = useState<RulesListResponse>(EMPTY_RULES);
  const [recoveryItems, setRecoveryItems] = useState<RecoveryEntry[]>([]);
  const [recentBatches, setRecentBatches] = useState<BatchSummary[]>([]);
  const [suggestedDownloads, setSuggestedDownloads] = useState<string | null>(null);
  const [firstRunDismissed, setFirstRunDismissed] = useState<boolean>(false);

  useTheme(settings.theme);

  const refreshAll = useCallback(async () => {
    if (!connected) {
      setLoading(false);
      return;
    }

    setError(null);
    try {
      const [info, status, cfg, folderList, rules, recovery, batches, downloads] =
        await Promise.all([
          ipc.getAppInfo(),
          ipc.getDatabaseStatus(),
          ipc.getSettings(),
          ipc.listWatchedFolders(),
          ipc.listRules(),
          ipc.listRecoveryItems(),
          ipc.listBatches({ offset: 0, limit: 10, statusFilter: null }),
          ipc.getSuggestedDownloadsFolder(),
        ]);

      setAppInfo(info);
      setDbStatus(status);
      setSettings(cfg);
      setFolders(folderList);
      setRulesData(rules);
      setRecoveryItems(recovery);
      setRecentBatches(batches.items);
      setSuggestedDownloads(downloads);
    } catch (err) {
      setError(resolveErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [connected]);

  useEffect(() => {
    if (!connected) {
      return undefined;
    }
    let cancelled = false;
    void Promise.all([
      ipc.getAppInfo(),
      ipc.getDatabaseStatus(),
      ipc.getSettings(),
      ipc.listWatchedFolders(),
      ipc.listRules(),
      ipc.listRecoveryItems(),
      ipc.listBatches({ offset: 0, limit: 10, statusFilter: null }),
      ipc.getSuggestedDownloadsFolder(),
    ])
      .then(([info, status, cfg, folderList, rules, recovery, batches, downloads]) => {
        if (cancelled) {
          return;
        }
        setError(null);
        setAppInfo(info);
        setDbStatus(status);
        setSettings(cfg);
        setFolders(folderList);
        setRulesData(rules);
        setRecoveryItems(recovery);
        setRecentBatches(batches.items);
        setSuggestedDownloads(downloads);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(resolveErrorMessage(err));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [connected]);

  const saveSettings = useCallback(
    async (next: AppSettings): Promise<AppSettings> => {
      if (!connected) {
        setSettings(next);
        return next;
      }
      const saved = await ipc.updateSettings(next);
      setSettings(saved);
      return saved;
    },
    [connected],
  );

  const dismissFirstRun = useCallback(() => {
    setFirstRunDismissed(true);
  }, []);

  const value = useMemo<AppDataContextValue>(
    () => ({
      isDesktopConnected: connected,
      loading,
      error,
      appInfo,
      dbStatus,
      settings,
      folders,
      rulesData,
      recoveryItems,
      recentBatches,
      suggestedDownloads,
      firstRunDismissed,
      dismissFirstRun,
      refreshAll,
      saveSettings,
      setFolders,
      setRulesData,
      setRecoveryItems,
    }),
    [
      connected,
      loading,
      error,
      appInfo,
      dbStatus,
      settings,
      folders,
      rulesData,
      recoveryItems,
      recentBatches,
      suggestedDownloads,
      firstRunDismissed,
      dismissFirstRun,
      refreshAll,
      saveSettings,
    ],
  );

  return <AppDataContext.Provider value={value}>{children}</AppDataContext.Provider>;
}

export function useAppData(): AppDataContextValue {
  const ctx = useContext(AppDataContext);
  if (!ctx) {
    throw new Error('useAppData must be used within AppDataProvider');
  }
  return ctx;
}
