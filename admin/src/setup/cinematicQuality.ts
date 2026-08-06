export type CinematicSceneDetail = 'full' | 'reduced';

export function getCinematicSceneDetail({
  deviceMemory,
  hardwareConcurrency,
  viewportWidth,
}: {
  deviceMemory?: number;
  hardwareConcurrency?: number;
  viewportWidth: number;
}): CinematicSceneDetail {
  const constrainedCpu = hardwareConcurrency !== undefined
    && hardwareConcurrency > 0
    && hardwareConcurrency <= 4;
  const constrainedMemory = deviceMemory !== undefined
    && deviceMemory > 0
    && deviceMemory <= 4;
  return viewportWidth < 640 || constrainedCpu || constrainedMemory ? 'reduced' : 'full';
}
