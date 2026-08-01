interface BrandMarkProps {
  className?: string;
  priority?: boolean;
}

export function BrandMark({ className = 'size-10', priority = false }: BrandMarkProps) {
  return (
    <img
      alt=""
      aria-hidden="true"
      className={`shrink-0 object-contain ${className}`}
      decoding="async"
      height="512"
      loading={priority ? 'eager' : 'lazy'}
      src="/brand/tjxy-mark.webp"
      width="512"
    />
  );
}
