import { Button, Label, ListBox, Select } from '@heroui/react';
import { Network, RefreshCw } from 'lucide-react';
import { getApiBaseUrl } from '../api/apiBase';
import {
  refreshStreamRoutes,
  selectStreamOrigin,
  setStreamRoutePreference,
  useStreamRoutes,
  type StreamRoute,
} from '../api/streamRoutes';
import { useTranslate } from '../../settings/i18n';

const AUTO_KEY = 'auto';

/**
 * Chooses the stream route for the running player. `origin` is the route the
 * player is pinned to; `onSwitch` reloads playback on another route.
 */
export function StreamRouteSelect({ origin, onSwitch }: { origin: string; onSwitch: (origin: string) => void }) {
  const tr = useTranslate();
  const { routes, preference, probing } = useStreamRoutes();
  if (routes.length < 2) return null;
  const routeName = (route: StreamRoute) => route.name ?? tr('Current address', '当前地址');
  const routeStatus = (route: StreamRoute) => {
    if (route.status === 'ok' && route.latencyMs !== undefined) return `${String(route.latencyMs)} ms`;
    if (route.status === 'failed') return tr('Unavailable', '不可用');
    return tr('Measuring…', '测速中…');
  };
  const playing = routes.find((route) => route.origin === origin);
  const autoLabel = playing
    ? tr(`Automatic (${routeName(playing)} · ${routeStatus(playing)})`, `自动（${routeName(playing)} · ${routeStatus(playing)}）`)
    : tr('Automatic', '自动');
  const selectedKey = preference.mode === 'auto' ? AUTO_KEY : preference.origin;

  return (
    <div className="flex items-end gap-2">
      <Select
        aria-label={tr('Stream route', '播放线路')}
        className="min-w-0 flex-1"
        value={selectedKey}
        variant="secondary"
        onChange={(key) => {
          if (key === null) return;
          const selected = String(key);
          const next = selected === AUTO_KEY ? { mode: 'auto' as const } : { mode: 'manual' as const, origin: selected };
          setStreamRoutePreference(next);
          onSwitch(selectStreamOrigin(routes, next, getApiBaseUrl()));
        }}
      >
        <Label className="flex items-center gap-2 text-sm">
          <Network className="size-4" />
          {tr('Stream route', '播放线路')}
        </Label>
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id={AUTO_KEY} textValue={autoLabel}>
              {autoLabel}
              <ListBox.ItemIndicator />
            </ListBox.Item>
            {routes.map((route) => (
              <ListBox.Item
                id={route.origin}
                isDisabled={route.status === 'failed'}
                key={route.origin}
                textValue={`${routeName(route)} ${routeStatus(route)}`}
              >
                <span className="min-w-0 truncate">{routeName(route)}</span>
                <span className="ml-auto pl-3 text-xs text-muted tabular-nums">{routeStatus(route)}</span>
                <ListBox.ItemIndicator />
              </ListBox.Item>
            ))}
          </ListBox>
        </Select.Popover>
      </Select>
      <Button
        aria-label={tr('Measure routes again', '重新测速')}
        isIconOnly
        isPending={probing}
        variant="secondary"
        onPress={() => {
          void refreshStreamRoutes().then((best) => {
            if (preference.mode === 'auto' && best !== origin) onSwitch(best);
          }).catch(() => undefined);
        }}
      >
        <RefreshCw className="size-4" />
      </Button>
    </div>
  );
}
