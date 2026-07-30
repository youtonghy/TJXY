import { Star } from 'lucide-react';
import { Link } from 'react-router-dom';
import type { MediaItem } from '../api/catalogApi';
import { MediaImage } from './MediaImage';

export function MediaTile({ item }: { item: MediaItem }) {
  const tag = item.ImageTags?.Primary ?? item.PrimaryImageTag;
  const episodeCode = item.Type === 'Episode' && item.IndexNumber !== undefined ? `E${String(item.IndexNumber)}` : undefined;
  const facts = [episodeCode, item.ProductionYear ? String(item.ProductionYear) : undefined].filter(Boolean);
  return (
    <Link className="group block min-w-0" to={`/app/items/${item.Id}`}>
      <div className="aspect-[2/3] overflow-hidden rounded-xl bg-default shadow-sm transition-transform group-hover:scale-[1.02]">
        <MediaImage alt={`Poster for ${item.Name}`} className="h-full w-full object-cover" itemId={item.Id} tag={tag} />
      </div>
      <p className="mt-2 truncate text-sm font-medium text-foreground">{item.Name}</p>
      {(facts.length > 0 || item.CommunityRating !== undefined) && (
        <div className="mt-0.5 flex items-center gap-2 text-xs text-muted">
          {facts.length > 0 && <span>{facts.join(' · ')}</span>}
          {item.CommunityRating !== undefined && (
            <span className="inline-flex items-center gap-1">
              <Star aria-hidden="true" className="size-3 text-accent" />
              {item.CommunityRating.toFixed(1)}
            </span>
          )}
        </div>
      )}
    </Link>
  );
}
