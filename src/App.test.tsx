import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { axe } from 'vitest-axe';
import { App } from './App';
import { messages } from './i18n';
import * as ipc from './services/ipc';
import type {
  AppInfo,
  AppSettings,
  BatchDetailResponse,
  BatchSummary,
  DatabaseStatusInfo,
  ExecutionResult,
  FolderStorageReport,
  PlanPageResponse,
  PlanSummary,
  RecoveryEntry,
  RulesListResponse,
  WatchedFolder,
} from './types/generated';

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue('D:\\Sandbox\\Inbox'),
}));

vi.mock('./services/ipc', () => ({
  getAppInfo: vi.fn(),
  getDatabaseStatus: vi.fn(),
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  listWatchedFolders: vi.fn(),
  addWatchedFolder: vi.fn(),
  updateWatchedFolder: vi.fn(),
  removeWatchedFolder: vi.fn(),
  listRules: vi.fn(),
  createRule: vi.fn(),
  updateRule: vi.fn(),
  deleteRule: vi.fn(),
  reorderRules: vi.fn(),
  restoreDefaultRules: vi.fn(),
  createDryRunPlan: vi.fn(),
  getPlanPage: vi.fn(),
  discardPlan: vi.fn(),
  executePlan: vi.fn(),
  cancelBatch: vi.fn(),
  undoBatch: vi.fn(),
  listBatches: vi.fn(),
  getBatchDetail: vi.fn(),
  searchMoveLogs: vi.fn(),
  analyzeFolderStorage: vi.fn(),
  listRecoveryItems: vi.fn(),
  acknowledgeRecoveryItem: vi.fn(),
  retryRecovery: vi.fn(),
  revealMoveLogInExplorer: vi.fn(),
  openLogsFolder: vi.fn(),
  getSuggestedDownloadsFolder: vi.fn(),
  exportRedactedDiagnostics: vi.fn(),
}));

const baseAppInfo: AppInfo = {
  version: '0.1.0',
  identifier: 'com.sortdeck.app',
  schemaVersion: 1,
  dataDirOverrideActive: true,
};

const baseDbStatus: DatabaseStatusInfo = {
  schemaVersion: 1,
  configRevision: 4,
  dbPath: 'D:\\Sandbox\\sortdeck-data\\sortdeck.db',
  dbSizeBytes: 65_536,
  recoveredFromCorruption: false,
  corruptBackupPath: null,
};

const baseSettings: AppSettings = {
  theme: 'dark',
  launchOnStartup: false,
  startMinimized: false,
  minimizeToTray: true,
  globalAutoWatch: true,
  defaultScanDepth: 1,
  retentionDays: 365,
  notifications: true,
};

const sampleFolder: WatchedFolder = {
  id: 1,
  path: 'D:\\Sandbox\\Downloads',
  pathKey: 'd:/sandbox/downloads',
  enabled: true,
  autoSort: false,
  recursive: false,
  maxDepth: 1,
  minFileAgeSecs: 5,
  status: 'ok',
  autoSortEnabledAt: null,
  lastReconciledAt: null,
  createdAt: 1_700_000_000_000,
  updatedAt: 1_700_000_000_000,
};

const sampleRules: RulesListResponse = {
  rules: [
    {
      id: 1,
      name: 'Images',
      destination: 'Images/',
      extensions: ['png', 'jpg'],
      enabled: true,
      priority: 1,
      isDefault: true,
      defaultKey: 'default.images',
      nameContains: null,
      minSizeBytes: null,
      maxSizeBytes: null,
      minAgeDays: null,
      createdAt: 1_700_000_000_000,
      updatedAt: 1_700_000_000_000,
    },
    {
      id: 2,
      name: 'Documents',
      destination: 'Documents/',
      extensions: ['pdf', 'txt'],
      enabled: true,
      priority: 2,
      isDefault: true,
      defaultKey: 'default.documents',
      nameContains: null,
      minSizeBytes: null,
      maxSizeBytes: null,
      minAgeDays: null,
      createdAt: 1_700_000_000_000,
      updatedAt: 1_700_000_000_000,
    },
  ],
  overlaps: [
    {
      extension: 'ts',
      winnerRuleId: 1,
      winnerRuleName: 'Images',
      shadowedRuleIds: [2],
    },
  ],
};

const sampleBatch: BatchSummary = {
  id: 'batch-001',
  kind: 'manual',
  sourceFolder: 'D:\\Sandbox\\Downloads',
  sourceFolderId: 1,
  parentBatchId: null,
  status: 'partial',
  totalFiles: 2,
  successfulMoves: 1,
  failedMoves: 1,
  skippedFiles: 0,
  totalBytes: 2048,
  createdAt: 1_700_000_500_000,
  completedAt: 1_700_000_505_000,
};

const sampleBatchDetail: BatchDetailResponse = {
  batch: sampleBatch,
  items: [
    {
      id: 10,
      batchId: 'batch-001',
      originalPath: 'D:\\Sandbox\\Downloads\\invoice\u202Efdp.exe',
      newPath: 'D:\\Sandbox\\Downloads\\Installers\\invoice\u202Efdp.exe',
      status: 'completed',
      crossVolume: false,
      sha256: null,
      fileSize: 2048,
      ruleId: 1,
      movedAt: 1_700_000_502_000,
      undone: false,
      undoneAt: null,
      undoBatchId: null,
      undoNote: null,
    },
  ],
};

const sampleRecovery: RecoveryEntry = {
  journalId: 77,
  batchId: 'batch-crash-77',
  operationType: 'move_cross_volume',
  state: 'recovery_required',
  sourcePath: 'D:\\Sandbox\\Downloads\\archive.zip',
  destinationPath: 'E:\\Archive\\archive.zip',
  temporaryPath: null,
  errorCode: 32,
  errorMessage: 'Sharing violation',
  detail: 'source_remains_after_verified_copy',
  updatedAt: 1_700_000_600_000,
};

const sampleStorageReport: FolderStorageReport = {
  folderId: 1,
  folderPath: 'D:\\Sandbox\\Downloads',
  totalFiles: 5,
  totalBytes: 12_582_912,
  oldFilesCount: 2,
  oldFilesBytes: 4_194_304,
  categories: [
    { name: 'Images', fileCount: 3, totalBytes: 8_388_608 },
    { name: 'Documents', fileCount: 2, totalBytes: 4_194_304 },
  ],
  largestFiles: [
    {
      path: 'D:\\Sandbox\\Downloads\\render.psd',
      fileName: 'render.psd',
      sizeBytes: 6_291_456,
      modifiedEpochMs: 1_700_000_000_000,
      matchedRuleName: 'Adobe Creative',
    },
  ],
  duplicateGroups: [
    {
      sha256: 'abc123def456',
      sizeBytes: 2_097_152,
      wastedBytes: 2_097_152,
      paths: ['D:\\Sandbox\\Downloads\\brochure.pdf', 'D:\\Sandbox\\Downloads\\brochure (1).pdf'],
    },
  ],
  scannedAt: 1_700_000_700_000,
};

function setupDefaultMocks() {
  vi.mocked(ipc.getAppInfo).mockResolvedValue(baseAppInfo);
  vi.mocked(ipc.getDatabaseStatus).mockResolvedValue(baseDbStatus);
  vi.mocked(ipc.getSettings).mockResolvedValue(baseSettings);
  vi.mocked(ipc.updateSettings).mockImplementation(async (next) => Promise.resolve(next));
  vi.mocked(ipc.listWatchedFolders).mockResolvedValue([sampleFolder]);
  vi.mocked(ipc.listRules).mockResolvedValue(sampleRules);
  vi.mocked(ipc.createRule).mockResolvedValue(sampleRules.rules[0]!);
  vi.mocked(ipc.listRecoveryItems).mockResolvedValue([]);
  vi.mocked(ipc.listBatches).mockResolvedValue({
    total: 1,
    offset: 0,
    limit: 50,
    items: [sampleBatch],
  });
  vi.mocked(ipc.getBatchDetail).mockResolvedValue(sampleBatchDetail);
  vi.mocked(ipc.searchMoveLogs).mockResolvedValue([]);
  vi.mocked(ipc.analyzeFolderStorage).mockResolvedValue(sampleStorageReport);
  vi.mocked(ipc.getSuggestedDownloadsFolder).mockResolvedValue(null);
  vi.mocked(ipc.discardPlan).mockResolvedValue(undefined);
  vi.mocked(ipc.reorderRules).mockResolvedValue(sampleRules);
  vi.mocked(ipc.restoreDefaultRules).mockResolvedValue(sampleRules);
  vi.mocked(ipc.undoBatch).mockResolvedValue({
    undoBatchId: 'undo-001',
    parentBatchId: 'batch-001',
    status: 'completed',
    restoredCount: 1,
    skippedCount: 0,
    failedCount: 0,
    removedEmptyDirs: 0,
    items: [],
  });
  vi.mocked(ipc.revealMoveLogInExplorer).mockResolvedValue(undefined);
  vi.mocked(ipc.acknowledgeRecoveryItem).mockResolvedValue(undefined);
}

describe('SortDeck Phase 4 Frontend Application', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
    document.documentElement.dataset.theme = 'dark';
  });

  it('passes automated axe checks across all 6 primary routes', async () => {
    const routes = ['/', '/folders', '/rules', '/history', '/recovery', '/settings'];
    for (const route of routes) {
      const { container, unmount } = render(<App initialRoute={route} forceIpc />);
      await waitFor(() => {
        expect(screen.getByRole('heading', { level: 1 })).toBeInTheDocument();
      });
      const results = await axe(container);
      expect(results).toHaveNoViolations();
      unmount();
    }
  });

  it('renders dry-run preview with selective deselect and faithful partial outcome (INV-14)', async () => {
    const plan: PlanSummary = {
      planId: 'plan-123',
      folderId: 1,
      folderPath: sampleFolder.path,
      createdAt: 1_700_000_000_000,
      expiresAt: 1_700_000_600_000,
      configRevision: 4,
      totalDiscovered: 2,
      eligibleCount: 2,
      conflictRenamedCount: 1,
      skippedCount: 0,
      totalBytesToMove: 4096,
      crossVolumeCount: 0,
      skipReasonCounts: [],
      destinationGroups: [
        {
          destinationDir: 'D:\\Sandbox\\Downloads\\Images',
          fileCount: 2,
          totalBytes: 4096,
        },
      ],
      warnings: [],
      sortignoreDetected: false,
    };

    const page: PlanPageResponse = {
      planId: 'plan-123',
      totalMatching: 2,
      offset: 0,
      limit: 200,
      items: [
        {
          operationId: 'op-1',
          sourcePath: 'D:\\Sandbox\\Downloads\\photo.png',
          destinationPath: 'D:\\Sandbox\\Downloads\\Images\\photo.png',
          originalFilename: 'photo.png',
          proposedFilename: 'photo.png',
          fileSize: 2048,
          sourceFingerprint: { volumeSerial: 1, fileId: '1', size: 2048, modifiedEpochMs: 100 },
          matchedRuleId: 1,
          matchedRuleName: 'Images',
          operationStatus: 'eligible',
          skipReason: null,
          crossVolumeEstimate: false,
          warnings: [],
        },
        {
          operationId: 'op-2',
          sourcePath: 'D:\\Sandbox\\Downloads\\pic.png',
          destinationPath: 'D:\\Sandbox\\Downloads\\Images\\pic (1).png',
          originalFilename: 'pic.png',
          proposedFilename: 'pic (1).png',
          fileSize: 2048,
          sourceFingerprint: { volumeSerial: 1, fileId: '2', size: 2048, modifiedEpochMs: 100 },
          matchedRuleId: 1,
          matchedRuleName: 'Images',
          operationStatus: 'conflict_renamed',
          skipReason: null,
          crossVolumeEstimate: false,
          warnings: [],
        },
      ],
    };

    const partialResult: ExecutionResult = {
      batchId: 'batch-partial-1',
      status: 'partial',
      totalPlanned: 1,
      succeeded: 0,
      failed: 1,
      skipped: 0,
      recoveryRequiredCount: 0,
      outcomes: [],
    };

    vi.mocked(ipc.createDryRunPlan).mockResolvedValue(plan);
    vi.mocked(ipc.getPlanPage).mockResolvedValue(page);
    vi.mocked(ipc.executePlan).mockResolvedValue(partialResult);

    const { container } = render(<App initialRoute="/folders" forceIpc />);
    const previewBtn = await screen.findByRole('button', { name: messages.folders.previewButton });
    fireEvent.click(previewBtn);

    const dialog = await screen.findByRole('dialog', { name: messages.preview.title });
    await within(dialog).findByText('photo.png');

    // Deselect second file before executing
    const secondCheckbox = within(dialog).getByRole('checkbox', {
      name: `${messages.preview.columnInclude}: pic.png`,
    });
    fireEvent.click(secondCheckbox);

    const modalAxe = await axe(container);
    expect(modalAxe).toHaveNoViolations();

    const confirmSortBtn = within(dialog).getByRole('button', {
      name: new RegExp(`${messages.preview.confirmSortButton} \\(1\\)`, 'i'),
    });
    fireEvent.click(confirmSortBtn);

    const confirmDialog = await screen.findByRole('dialog', {
      name: messages.preview.confirmDialogTitle,
    });
    fireEvent.click(
      within(confirmDialog).getByRole('button', { name: messages.preview.confirmSortButton }),
    );

    await waitFor(() => {
      expect(ipc.executePlan).toHaveBeenCalledWith({
        planId: 'plan-123',
        deselectedOperationIds: ['op-2'],
      });
    });

    const partialTitle = await within(dialog).findByText(messages.preview.resultTitlePartial);
    expect(partialTitle).toBeInTheDocument();
  });

  it('supports rule keyboard reordering, overlap display, and restore defaults confirmation', async () => {
    render(<App initialRoute="/rules" forceIpc />);

    expect(await screen.findByText(messages.rules.overlapTitle)).toBeInTheDocument();

    const moveDownBtn = screen.getByRole('button', {
      name: `${messages.rules.moveDownAria}: Images`,
    });
    fireEvent.click(moveDownBtn);

    await waitFor(() => {
      expect(ipc.reorderRules).toHaveBeenCalledWith({ orderedRuleIds: [2, 1] });
    });

    fireEvent.click(screen.getByRole('button', { name: messages.rules.restoreDefaultsButton }));
    const dialog = await screen.findByRole('dialog', { name: messages.rules.restoreDialogTitle });
    expect(within(dialog).getByText(messages.rules.restoreWillNotHappen)).toBeInTheDocument();
  });

  it('sanitizes bidi filenames in History and uses ID-based IPC for Reveal and Undo', async () => {
    render(<App initialRoute="/history" forceIpc />);

    await waitFor(() => {
      expect(screen.getAllByText(/invoice\[U\+202E\]fdp\.exe/i)).toHaveLength(2);
    });

    const revealBtn = screen.getByRole('button', {
      name: new RegExp(messages.history.revealInExplorer, 'i'),
    });
    fireEvent.click(revealBtn);
    await waitFor(() => {
      expect(ipc.revealMoveLogInExplorer).toHaveBeenCalledWith(10);
    });

    fireEvent.click(screen.getByRole('button', { name: messages.history.undoBatchButton }));
    const undoDialog = await screen.findByRole('dialog', {
      name: messages.history.undoDialogTitle,
    });
    fireEvent.click(
      within(undoDialog).getByRole('button', { name: messages.history.undoBatchButton }),
    );

    await waitFor(() => {
      expect(ipc.undoBatch).toHaveBeenCalledWith({
        batchId: 'batch-001',
        itemIds: null,
      });
    });
  });

  it('surfaces recovery banner and allows acknowledging recovery items cleanly', async () => {
    vi.mocked(ipc.listRecoveryItems).mockResolvedValue([sampleRecovery]);

    render(<App initialRoute="/" forceIpc />);

    expect(
      await screen.findByText(`${messages.alerts.recoveryBannerTitle} (1)`),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('link', { name: messages.alerts.recoveryBannerAction }));
    expect(await screen.findByText(/source_remains_after_verified_copy/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: messages.recovery.acknowledgeButton }));
    const ackDialog = await screen.findByRole('dialog', { name: messages.recovery.ackDialogTitle });
    fireEvent.click(
      within(ackDialog).getByRole('button', { name: messages.recovery.acknowledgeButton }),
    );

    await waitFor(() => {
      expect(ipc.acknowledgeRecoveryItem).toHaveBeenCalledWith(77);
    });
  });

  it('switches themes immediately and persists Settings updates', async () => {
    render(<App initialRoute="/settings" forceIpc />);

    const lightRadio = await screen.findByRole('radio', { name: messages.settings.themeLight });
    fireEvent.click(lightRadio);

    await waitFor(() => {
      expect(ipc.updateSettings).toHaveBeenCalledWith({
        ...baseSettings,
        theme: 'light',
      });
      expect(document.documentElement.dataset.theme).toBe('light');
    });
  });

  it('searches moved file history from Dashboard, reveals in Explorer, and supports Put Back', async () => {
    vi.mocked(ipc.searchMoveLogs).mockResolvedValue(sampleBatchDetail.items);

    render(<App initialRoute="/" forceIpc />);

    const input = await screen.findByLabelText(messages.fileSearch.inputLabel);
    fireEvent.change(input, { target: { value: 'invoice' } });
    fireEvent.click(screen.getByRole('button', { name: messages.fileSearch.searchButton }));

    await waitFor(() => {
      expect(ipc.searchMoveLogs).toHaveBeenCalledWith('invoice', 20);
    });

    await waitFor(() => {
      expect(screen.getAllByText(/invoice\[U\+202E\]fdp\.exe/i)).toHaveLength(2);
    });

    fireEvent.click(screen.getByRole('button', { name: messages.fileSearch.showInExplorerButton }));
    await waitFor(() => {
      expect(ipc.revealMoveLogInExplorer).toHaveBeenCalledWith(10);
    });

    fireEvent.click(screen.getByRole('button', { name: messages.fileSearch.putBackButton }));
    const undoDialog = await screen.findByRole('dialog', {
      name: messages.history.undoDialogTitle,
    });
    fireEvent.click(
      within(undoDialog).getByRole('button', { name: messages.fileSearch.putBackButton }),
    );

    await waitFor(() => {
      expect(ipc.undoBatch).toHaveBeenCalledWith({
        batchId: 'batch-001',
        itemIds: [10],
      });
    });
  });

  it('opens Starter Packs modal cleanly and saves smart rule filters with date tokens', async () => {
    const { container } = render(<App initialRoute="/rules" forceIpc />);

    const packsButton = await screen.findByRole('button', {
      name: messages.rules.starterPacksButton,
    });
    fireEvent.click(packsButton);

    const packsDialog = await screen.findByRole('dialog', {
      name: messages.starterPacks.title,
    });
    const packsAxe = await axe(container);
    expect(packsAxe).toHaveNoViolations();

    const addButtons = within(packsDialog).getAllByRole('button', {
      name: messages.starterPacks.addPackButton,
    });
    fireEvent.click(addButtons[0]!);

    await waitFor(() => {
      expect(ipc.createRule).toHaveBeenCalled();
      expect(
        screen.queryByRole('dialog', { name: messages.starterPacks.title }),
      ).not.toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: messages.rules.newRuleButton }));
    const editorDialog = await screen.findByRole('dialog', {
      name: messages.rules.editorCreateTitle,
    });

    fireEvent.change(within(editorDialog).getByLabelText(messages.rules.fieldNameLabel), {
      target: { value: 'Receipts' },
    });
    fireEvent.change(within(editorDialog).getByLabelText(messages.rules.fieldDestinationLabel), {
      target: { value: 'Documents/' },
    });
    fireEvent.click(
      within(editorDialog).getByRole('button', { name: messages.rules.dateTokenYear }),
    );

    expect(within(editorDialog).getByLabelText(messages.rules.fieldDestinationLabel)).toHaveValue(
      'Documents/{year}/',
    );

    fireEvent.change(within(editorDialog).getByLabelText(messages.rules.fieldExtensionsLabel), {
      target: { value: 'pdf' },
    });
    fireEvent.change(within(editorDialog).getByLabelText(messages.rules.fieldNameContainsLabel), {
      target: { value: 'Receipt' },
    });
    fireEvent.change(within(editorDialog).getByLabelText(messages.rules.fieldMinSizeMbLabel), {
      target: { value: '2' },
    });

    const editorAxe = await axe(container);
    expect(editorAxe).toHaveNoViolations();

    fireEvent.click(within(editorDialog).getByRole('button', { name: messages.common.save }));

    await waitFor(() => {
      expect(ipc.createRule).toHaveBeenLastCalledWith({
        name: 'Receipts',
        destination: 'Documents/{year}/',
        extensions: ['pdf'],
        enabled: true,
        nameContains: 'Receipt',
        minSizeBytes: 2 * 1024 * 1024,
        maxSizeBytes: null,
        minAgeDays: null,
      });
    });
  });

  it('runs read-only Check Space folder analysis and renders duplicate groups accessibly', async () => {
    const { container } = render(<App initialRoute="/folders" forceIpc />);

    const checkSpaceBtn = await screen.findByRole('button', {
      name: `${messages.insights.checkSpaceButton}: ${sampleFolder.path}`,
    });
    fireEvent.click(checkSpaceBtn);

    await waitFor(() => {
      expect(ipc.analyzeFolderStorage).toHaveBeenCalledWith(1);
    });

    const insightsDialog = await screen.findByRole('dialog', {
      name: messages.insights.title,
    });
    expect(within(insightsDialog).getByText(messages.insights.categoriesTitle)).toBeInTheDocument();
    expect(within(insightsDialog).getByText(messages.insights.duplicatesTitle)).toBeInTheDocument();
    expect(within(insightsDialog).getByText(/brochure\.pdf/i)).toBeInTheDocument();

    const insightsAxe = await axe(container);
    expect(insightsAxe).toHaveNoViolations();
  });
});
