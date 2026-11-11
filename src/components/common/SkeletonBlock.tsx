export function SkeletonBlock({ rows = 4 }: { rows?: number | undefined }) {
  return (
    <div className="space-y-3" aria-busy="true">
      {Array.from({ length: rows }).map((_, idx) => (
        <div
          key={idx}
          className="h-12 animate-pulse rounded-lg border border-border-subtle bg-surface-inset"
        />
      ))}
    </div>
  );
}
