import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { messages } from '../../i18n';
import type { BatchStatus } from '../../types/generated';
import { BatchStatusBadge } from './StatusBadge';

describe('BatchStatusBadge INV-14 fidelity', () => {
  const nonSuccessStatuses: readonly BatchStatus[] = [
    'partial',
    'failed',
    'cancelled',
    'recovery_required',
  ];

  it('never renders success tone or wording for non-success statuses', () => {
    for (const status of nonSuccessStatuses) {
      const { container, unmount } = render(<BatchStatusBadge status={status} />);
      const badge = container.querySelector('[data-status-kind="batch"]');
      expect(badge).not.toBeNull();
      expect(badge?.getAttribute('data-semantic-tone')).not.toBe('success');
      expect(screen.getByText(messages.statuses.batch[status])).toBeInTheDocument();
      expect(screen.queryByText(messages.statuses.batch.completed)).not.toBeInTheDocument();
      unmount();
    }
  });

  it('maps each BatchStatus to a distinct semantic tone and accessible label', () => {
    const seenTones = new Set<string>();
    const all: readonly BatchStatus[] = [
      'completed',
      'partial',
      'failed',
      'cancelled',
      'recovery_required',
      'running',
    ];

    for (const status of all) {
      const { container, unmount } = render(<BatchStatusBadge status={status} />);
      const badge = container.querySelector('[data-status-kind="batch"]');
      const tone = badge?.getAttribute('data-semantic-tone') ?? '';
      seenTones.add(tone);
      unmount();
    }

    expect(seenTones.size).toBe(all.length);
  });
});
