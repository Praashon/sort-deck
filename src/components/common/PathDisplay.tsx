import { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { messages } from '../../i18n';
import { sanitizeFilenameForDisplay, truncateMiddle } from '../../utils/paths';

interface PathDisplayProps {
  path: string;
  maxLength?: number | undefined;
  showCopy?: boolean | undefined;
  className?: string | undefined;
}

export function PathDisplay({
  path,
  maxLength = 64,
  showCopy = true,
  className = '',
}: PathDisplayProps) {
  const [copied, setCopied] = useState(false);
  const sanitizedFull = sanitizeFilenameForDisplay(path);
  const display = truncateMiddle(path, maxLength);

  const handleCopy = () => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(path).then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1800);
      });
    }
  };

  return (
    <span className={`inline-flex min-w-0 max-w-full items-center gap-1.5 ${className}`}>
      <span title={sanitizedFull} className="truncate font-mono text-xs text-ink-secondary">
        {display}
      </span>
      {showCopy && (
        <button
          type="button"
          onClick={handleCopy}
          aria-label={`${messages.common.copyPath}: ${sanitizedFull}`}
          title={copied ? messages.common.copied : messages.common.copyPath}
          className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded border border-transparent text-ink-muted transition-colors hover:border-border-subtle hover:bg-surface-hover hover:text-ink"
        >
          {copied ? (
            <Check className="h-3 w-3 text-success-ink" aria-hidden="true" />
          ) : (
            <Copy className="h-3 w-3" aria-hidden="true" />
          )}
        </button>
      )}
    </span>
  );
}
