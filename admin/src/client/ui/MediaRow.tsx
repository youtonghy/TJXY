import { Link } from 'react-router-dom';
import type { MediaItem } from '../api/catalogApi';
import { MediaTile } from './MediaTile';
export function MediaRow({ title, items, moreTo }: { title: string; items: MediaItem[]; moreTo?: string }) { if (!items.length) return null; return <section aria-labelledby={`row-${title}`} className="space-y-3"><div className="flex items-center justify-between"><h2 className="text-lg font-semibold text-foreground" id={`row-${title}`}>{title}</h2>{moreTo && <Link className="text-sm text-accent hover:underline" to={moreTo}>View all</Link>}</div><div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">{items.map((item) => <MediaTile item={item} key={item.Id} />)}</div></section>; }
