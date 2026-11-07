interface BrandLogoProps {
  size?: number | undefined;
  className?: string | undefined;
}

/**
 * Custom SortDeck architectural emblem: three aligned deck cards locked by a
 * solid mineral-teal index bar, rendered without gradients or blurs.
 */
export function BrandLogo({ size = 28, className = '' }: BrandLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      className={className}
    >
      <rect x="1.5" y="1.5" width="29" height="29" rx="7" fill="#141417" stroke="#2e2e36" />
      <rect x="7" y="7" width="14" height="4" rx="1.5" fill="#464652" />
      <rect x="7" y="12.5" width="17" height="4" rx="1.5" fill="#7c7c8b" />
      <path
        d="M7 18C7 17.1716 7.67157 16.5 8.5 16.5H15L17 18.5H23.5C24.3284 18.5 25 19.1716 25 20V23.5C25 24.3284 24.3284 25 23.5 25H8.5C7.67157 25 7 24.3284 7 23.5V18Z"
        fill="#f4f4f5"
      />
      <rect x="21" y="7" width="4" height="9" rx="1.5" fill="#2cb5a0" />
    </svg>
  );
}
