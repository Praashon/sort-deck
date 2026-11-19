import { useEffect, useId, useState } from 'react';
import { Check, Layers, Plus, X } from 'lucide-react';
import { messages } from '../../i18n';
import type { CreateRuleRequest, Rule } from '../../types/generated';

export interface StarterPackDefinition {
  id: string;
  title: string;
  description: string;
  rules: readonly CreateRuleRequest[];
}

export const STARTER_PACKS: readonly StarterPackDefinition[] = [
  {
    id: 'adobe-split',
    title: messages.starterPacks.packs.adobeSplitTitle,
    description: messages.starterPacks.packs.adobeSplitDesc,
    rules: [
      {
        name: 'Adobe Photoshop',
        destination: 'Adobe/Photoshop/',
        extensions: ['psd', 'psb', 'abr'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
      {
        name: 'Adobe Illustrator',
        destination: 'Adobe/Illustrator/',
        extensions: ['ai', 'eps', 'ase'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
      {
        name: 'Adobe Premiere & After Effects',
        destination: 'Adobe/Video/',
        extensions: ['prproj', 'aep', 'aet', 'mogrt', 'sesx'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
      {
        name: 'Adobe InDesign & Lightroom',
        destination: 'Adobe/Publishing/',
        extensions: ['indd', 'indt', 'idml', 'lrcat', 'lrtemplate', 'xmp'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
    ],
  },
  {
    id: 'screenshots-receipts',
    title: messages.starterPacks.packs.screenshotsTitle,
    description: messages.starterPacks.packs.screenshotsDesc,
    rules: [
      {
        name: 'Screenshots by Month',
        destination: 'Images/Screenshots/{year}/{month}/',
        extensions: ['png', 'jpg', 'jpeg', 'webp'],
        enabled: true,
        nameContains: 'Screenshot',
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
      {
        name: 'Invoices & Receipts',
        destination: 'Documents/Finance/{year}/',
        extensions: ['pdf'],
        enabled: true,
        nameContains: 'Invoice',
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
    ],
  },
  {
    id: 'camera-raw',
    title: messages.starterPacks.packs.cameraRawTitle,
    description: messages.starterPacks.packs.cameraRawDesc,
    rules: [
      {
        name: 'Camera RAW',
        destination: 'Photos/RAW/{year}/{month}/',
        extensions: ['cr3', 'cr2', 'nef', 'arw', 'dng', 'raf', 'orf', 'rw2'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
    ],
  },
  {
    id: 'creator-3d-audio',
    title: messages.starterPacks.packs.creator3dAudioTitle,
    description: messages.starterPacks.packs.creator3dAudioDesc,
    rules: [
      {
        name: '3D & CAD Models',
        destination: 'Studio/3D/',
        extensions: ['blend', 'obj', 'fbx', 'stl', 'glb', 'gltf', 'step', 'stp'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
      {
        name: 'Music DAW Projects',
        destination: 'Studio/AudioProjects/',
        extensions: ['als', 'flp', 'ptx', 'logicx', 'rpp'],
        enabled: true,
        nameContains: null,
        minSizeBytes: null,
        maxSizeBytes: null,
        minAgeDays: null,
      },
    ],
  },
];

interface StarterPacksModalProps {
  open: boolean;
  existingRules: readonly Rule[];
  onInstallPack: (pack: StarterPackDefinition) => Promise<void>;
  onClose: () => void;
}

export function StarterPacksModal({
  open,
  existingRules,
  onInstallPack,
  onClose,
}: StarterPacksModalProps) {
  const titleId = useId();
  const [installingPackId, setInstallingPackId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && installingPackId === null) {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [open, installingPackId, onClose]);

  if (!open) {
    return null;
  }

  const existingNames = new Set(existingRules.map((r) => r.name.trim().toLowerCase()));

  const handleInstall = async (pack: StarterPackDefinition) => {
    setInstallingPackId(pack.id);
    try {
      await onInstallPack(pack);
    } finally {
      setInstallingPackId(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border-strong bg-surface-elevated shadow-xl"
      >
        <div className="flex items-start justify-between gap-3 border-b border-border-subtle px-5 py-4">
          <div>
            <h2 id={titleId} className="text-sm font-semibold text-ink">
              {messages.starterPacks.title}
            </h2>
            <p className="mt-0.5 text-xs text-ink-muted">{messages.starterPacks.description}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={installingPackId !== null}
            aria-label={messages.starterPacks.closeAriaLabel}
            className="rounded p-1 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-5">
          {STARTER_PACKS.map((pack) => {
            const allAlreadyAdded = pack.rules.every((r) =>
              existingNames.has(r.name.trim().toLowerCase()),
            );
            const busy = installingPackId === pack.id;

            return (
              <section
                key={pack.id}
                className="rounded-lg border border-border-subtle bg-surface p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <Layers className="h-4 w-4 text-accent" aria-hidden="true" />
                      <h3 className="text-xs font-semibold text-ink">{pack.title}</h3>
                    </div>
                    <p className="mt-1 text-xs leading-relaxed text-ink-muted">
                      {pack.description}
                    </p>
                  </div>

                  {allAlreadyAdded ? (
                    <span className="inline-flex items-center gap-1 rounded border border-success-border bg-success-bg px-2.5 py-1 text-[11px] font-medium text-success-ink">
                      <Check className="h-3 w-3" aria-hidden="true" />
                      <span>{messages.starterPacks.addedBadge}</span>
                    </span>
                  ) : (
                    <button
                      type="button"
                      disabled={installingPackId !== null}
                      onClick={() => void handleInstall(pack)}
                      className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
                    >
                      <Plus className="h-3.5 w-3.5" aria-hidden="true" />
                      <span>
                        {busy ? messages.starterPacks.adding : messages.starterPacks.addPackButton}
                      </span>
                    </button>
                  )}
                </div>

                <div className="mt-3 flex flex-wrap gap-1.5 border-t border-border-subtle pt-2.5">
                  {pack.rules.map((r) => (
                    <span
                      key={r.name}
                      className="rounded border border-border-subtle bg-surface-inset px-2 py-0.5 text-[11px] text-ink-secondary"
                    >
                      {r.name} -&gt; <span className="font-mono">{r.destination}</span>
                    </span>
                  ))}
                </div>
              </section>
            );
          })}
        </div>

        <div className="flex justify-end border-t border-border-subtle bg-surface px-5 py-3">
          <button
            type="button"
            onClick={onClose}
            disabled={installingPackId !== null}
            className="rounded-md border border-border-subtle bg-surface-elevated px-3 py-1.5 text-xs font-medium text-ink hover:bg-surface-hover"
          >
            {messages.common.close}
          </button>
        </div>
      </div>
    </div>
  );
}
