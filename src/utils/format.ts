import type { BatchDetailResponse, BatchSummary } from '../types/generated';

const byteUnits = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

const intFormatter = new Intl.NumberFormat(undefined, {
  maximumFractionDigits: 0,
});

const dateTimeFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
});

export function formatNumber(value: number): string {
  return intFormatter.format(value);
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B';
  }
  let unitIdx = 0;
  let scaled = bytes;
  while (scaled >= 1024 && unitIdx < byteUnits.length - 1) {
    scaled /= 1024;
    unitIdx += 1;
  }
  const unit = byteUnits[unitIdx] ?? 'B';
  const digits = unitIdx === 0 ? 0 : scaled >= 100 ? 0 : 1;
  const formatted = new Intl.NumberFormat(undefined, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(scaled);
  return `${formatted} ${unit}`;
}

export function formatTimestamp(epochMs: number | null): string {
  if (epochMs === null || !Number.isFinite(epochMs) || epochMs <= 0) {
    return '-';
  }
  return dateTimeFormatter.format(new Date(epochMs));
}

function escapeCsvCell(value: string | number | boolean | null): string {
  if (value === null) {
    return '';
  }
  const str = String(value);
  if (str.includes(',') || str.includes('"') || str.includes('\n') || str.includes('\r')) {
    return `"${str.replace(/"/g, '""')}"`;
  }
  return str;
}

export function buildBatchesCsv(
  batches: readonly BatchSummary[],
  selectedDetail?: BatchDetailResponse | null,
): string {
  const header = [
    'batch_id',
    'kind',
    'status',
    'source_folder',
    'total_files',
    'successful_moves',
    'failed_moves',
    'skipped_files',
    'total_bytes',
    'created_at',
    'completed_at',
  ];

  const rows = batches.map((b) =>
    [
      b.id,
      b.kind,
      b.status,
      b.sourceFolder,
      b.totalFiles,
      b.successfulMoves,
      b.failedMoves,
      b.skippedFiles,
      b.totalBytes,
      b.createdAt,
      b.completedAt,
    ]
      .map(escapeCsvCell)
      .join(','),
  );

  const sections = [header.join(','), ...rows];

  if (selectedDetail && selectedDetail.items.length > 0) {
    sections.push('');
    sections.push(
      [
        'move_log_id',
        'batch_id',
        'original_path',
        'new_path',
        'status',
        'cross_volume',
        'file_size',
        'moved_at',
        'undone',
        'undo_note',
      ].join(','),
    );
    for (const item of selectedDetail.items) {
      sections.push(
        [
          item.id,
          item.batchId,
          item.originalPath,
          item.newPath,
          item.status,
          item.crossVolume,
          item.fileSize,
          item.movedAt,
          item.undone,
          item.undoNote,
        ]
          .map(escapeCsvCell)
          .join(','),
      );
    }
  }

  return sections.join('\r\n');
}

export function downloadTextFile(filename: string, content: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}
