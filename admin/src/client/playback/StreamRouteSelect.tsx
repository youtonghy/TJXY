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
 * Chooses the stream route. In the player, `origin` is the route playback is
 * pinned to and `onSwitch` reloads it elsewhere; without them the control
 * edits the saved preference and reflects the currently active route.
 * `showSingle` keeps the control visible with a hint when there is only one
 * route, so the list can still be refreshed.
 */
export function StreamRouteSelect({
  origin,
  onSwitch,
  showSingle = false,
}: {
  origin?: string;
  onSwitch?: (origin: string) => void;
  showSingle?: boolean;
}) {
  const tr = useTranslate();
  const { routes, preference, probing, activeOrigin } = useStreamRoutes();
  const remeasure = () => {
    void refreshStreamRoutes().then((best) => {
      if (onSwitch && preference.mode === 'auto' && best !== origin) onSwitch(best);
    }).catch(() => undefined);
  };
  const refreshButton = (
    <Button
      aria-label={tr('Measure routes again', '重新测速')}
      isIconOnly
      isPending={probing}
      variant="secondary"
      onPress={remeasure}
    >
      <RefreshCw className="size-4" />
    </Button>
  );
  if (routes.length < 2) {
    if (!showSingle) return null;
    return (
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-muted">
          {probing
            ? tr('Measuring routes…', '正在测速…')
            : tr('The server has no other stream addresses; the current address is used.', '服务器未配置其他流地址，将使用当前地址。')}
        </p>
        {refreshButton}
      </div>
    );
  }
  const shown = origin ?? activeOrigin;
  const routeName = (route: StreamRoute) => route.name ?? tr('Current address', '当前地址');
  const routeStatus = (route: StreamRoute) => {
    if (route.status === 'ok' && route.latencyMs !== undefined) return `${String(route.latencyMs)} ms`;
    if (route.status === 'failed') return tr('Unavailable', '不可用');
    return tr('Measuring…', '测速中…');
  };
  const playing = routes.find((route) => route.origin === shown);
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
          onSwitch?.(selectStreamOrigin(routes, next, getApiBaseUrl()));
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
      {refreshButton}
    </div>
  );
}
