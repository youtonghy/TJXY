import type { ReactNode } from 'react';

export interface ResponsiveCollectionProps {
  ariaLabel: string;
  desktop: ReactNode;
  mobile: ReactNode;
}

export function ResponsiveCollection({ ariaLabel, desktop, mobile }: ResponsiveCollectionProps) {
  return (
    <>
      <div aria-label={ariaLabel} className="hidden sm:block" data-slot="desktop-collection" role="region">
        {desktop}
      </div>
      <div aria-label={ariaLabel} className="block sm:hidden" data-slot="mobile-collection" role="region">
        {mobile}
      </div>
    </>
  );
}
