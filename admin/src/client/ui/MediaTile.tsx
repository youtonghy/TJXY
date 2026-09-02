import { ProgressCircle } from '@heroui/react';
import { Check, Heart, Star } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useTranslate } from '../../settings/i18n';
import type { MediaItem } from '../api/catalogApi';
import { MediaImage } from './MediaImage';

export function MediaTile({ item, to, libraryId }: { item: MediaItem; to?: string; libraryId?: string }) {
  const tr = useTranslate();
  const tag = item.ImageTags?.Primary ?? item.PrimaryImageTag;
  const episodeCode = item.Type === 'Episode' && item.IndexNumber !== undefined ? `E${String(item.IndexNumber)}` : undefined;
  const facts = [episodeCode, item.ProductionYear ? String(item.ProductionYear) : undefined].filter(Boolean);
  const progress = watchedProgress(item);
  const artworkRatio = item.Type === 'Audio' ? 'aspect-square' : 'aspect-[2/3]';
  return (
    <Link className="group block min-w-0" to={to ?? `/app/items/${item.Id}${libraryId ? `?libraryId=${encodeURIComponent(libraryId)}` : ''}`}>
      <div className={`relative ${artworkRatio} overflow-hidden rounded-xl bg-default shadow-sm transition-transform group-hover:scale-[1.02]`}>
        <MediaImage alt={tr(`Poster for ${item.Name}`, `${item.Name} 的海报`)} className="h-full w-full object-cover" itemId={item.Id} libraryId={libraryId} tag={tag} />
        <div className="absolute right-2 top-2 flex items-center gap-1.5">
          {item.UserData?.IsFavorite && (
            <span
              aria-label={tr('Favorite', '已收藏')}
              className="grid size-7 place-items-center rounded-full bg-background/90 shadow-sm"
            >
              <Heart aria-hidden="true" className="size-4 fill-pink-500 text-pink-500" />
            </span>
          )}
          {item.UserData?.Played
            ? (
                <span
                  aria-label={tr('Watched', '已看完')}
                  className="grid size-7 place-items-center rounded-full bg-success text-success-foreground shadow-sm"
                >
                  <Check aria-hidden="true" className="size-4" strokeWidth={3} />
                </span>
              )
            : progress !== undefined
              ? (
                  <span className="grid size-7 place-items-center rounded-full bg-background/90 shadow-sm">
                    <ProgressCircle
                      aria-label={tr(`${String(progress)}% watched`, `已观看 ${String(progress)}%`)}
                      color="success"
                      size="sm"
                      value={progress}
                    >
                      <ProgressCircle.Track>
                        <ProgressCircle.TrackCircle />
                        <ProgressCircle.FillCircle />
                      </ProgressCircle.Track>
                    </ProgressCircle>
                  </span>
                )
              : null}
        </div>
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

function watchedProgress(item: MediaItem): number | undefined {
  const position = item.UserData?.PlaybackPositionTicks ?? 0;
  const runtime = item.RunTimeTicks ?? 0;
  if (position <= 0 || runtime <= 0 || !Number.isFinite(position) || !Number.isFinite(runtime)) {
    return undefined;
  }
  return Math.max(1, Math.min(99, Math.round((position / runtime) * 100)));
}
