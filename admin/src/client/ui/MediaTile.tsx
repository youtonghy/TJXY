import { Link } from 'react-router-dom';
import type { MediaItem } from '../api/catalogApi';
import { MediaImage } from './MediaImage';
export function MediaTile({ item }: { item: MediaItem }) { const tag = item.ImageTags?.Primary; return <Link className="group block min-w-0" to={`/app/items/${item.Id}`}><div className="aspect-[2/3] overflow-hidden rounded-xl bg-default shadow-sm transition-transform group-hover:scale-[1.02]"><MediaImage alt={`Poster for ${item.Name}`} className="h-full w-full object-cover" itemId={item.Id} tag={tag} /></div><p className="mt-2 truncate text-sm font-medium text-foreground">{item.Name}</p>{item.ProductionYear && <p className="text-xs text-muted">{item.ProductionYear}</p>}</Link>; }
