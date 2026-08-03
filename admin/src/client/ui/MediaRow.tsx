import { useId } from 'react';
import { Link } from 'react-router-dom';
import type { MediaItem } from '../api/catalogApi';
import { MediaTile } from './MediaTile';
import { useTranslate } from '../../settings/i18n';

interface MediaRowProps {
  title: string;
  items: MediaItem[];
  moreTo?: string;
  itemTo?: (item: MediaItem) => string;
  limitToTwoRows?: boolean;
}

export function MediaRow({ title, items, moreTo, itemTo, limitToTwoRows = false }: MediaRowProps) {
  const headingId = useId();
  const tr = useTranslate();
  if (!items.length) return null;
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-lg font-semibold text-foreground" id={headingId}>{title}</h2>
        {moreTo && <Link className="shrink-0 text-sm text-accent hover:underline" to={moreTo}>{tr('View all', '查看全部')}</Link>}
      </div>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
        {items.map((item, index) => (
          <div className={limitToTwoRows ? twoRowVisibility(index) : undefined} key={item.Id}>
            <MediaTile item={item} to={itemTo?.(item)} />
          </div>
        ))}
      </div>
    </section>
  );
}

function twoRowVisibility(index: number): string {
  if (index < 4) return '';
  if (index < 8) return 'hidden sm:block';
  if (index < 12) return 'hidden lg:block';
  return 'hidden';
}
