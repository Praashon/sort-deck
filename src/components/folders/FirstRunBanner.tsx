import { FolderPlus, X } from 'lucide-react';
import { messages } from '../../i18n';
import { BrandLogo } from '../common/BrandLogo';

interface FirstRunBannerProps {
  suggestedDownloads: string | null;
  busy: boolean;
  onAddDownloads: (path: string) => void;
  onChooseFolder: () => void;
  onDismiss: () => void;
}

export function FirstRunBanner({
  suggestedDownloads,
  busy,
  onAddDownloads,
  onChooseFolder,
  onDismiss,
}: FirstRunBannerProps) {
  return (
    <section
      aria-labelledby="first-run-title"
      className="rounded-lg border border-border-subtle bg-surface-elevated p-5"
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <BrandLogo size={32} className="mt-0.5 shrink-0" />
          <div>
            <p className="text-[11px] font-medium tracking-wider text-accent uppercase">
              {messages.firstRun.eyebrow}
            </p>
            <h2 id="first-run-title" className="mt-0.5 text-base font-semibold text-ink">
              {messages.firstRun.title}
            </h2>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-ink-muted">
              {messages.firstRun.description}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          aria-label={messages.firstRun.dismissAction}
          className="rounded p-1 text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink"
        >
          <X className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-2 border-y border-border-subtle py-3 text-xs text-ink-secondary md:grid-cols-3">
        <p>{messages.firstRun.bulletNoOverwrite}</p>
        <p>{messages.firstRun.bulletPreviewFirst}</p>
        <p>{messages.firstRun.bulletUndoHistory}</p>
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-2">
        {suggestedDownloads && (
          <button
            type="button"
            disabled={busy}
            onClick={() => onAddDownloads(suggestedDownloads)}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
          >
            <FolderPlus className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{messages.firstRun.addDownloadsAction}</span>
          </button>
        )}
        <button
          type="button"
          disabled={busy}
          onClick={onChooseFolder}
          className="inline-flex items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
        >
          <span>{messages.firstRun.chooseFolderAction}</span>
        </button>
      </div>
    </section>
  );
}
