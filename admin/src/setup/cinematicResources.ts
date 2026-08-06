export interface DisposableResource { dispose: () => void }

export interface CinematicResourceScope {
  dispose: () => void;
  track: <T extends DisposableResource>(resource: T) => T;
}

export function createCinematicResourceScope(): CinematicResourceScope {
  const resources: DisposableResource[] = [];
  const tracked = new Set<DisposableResource>();
  let disposed = false;

  return {
    dispose: () => {
      if (disposed) return;
      disposed = true;
      for (let index = resources.length - 1; index >= 0; index -= 1) resources[index]?.dispose();
      resources.length = 0;
      tracked.clear();
    },
    track: <T extends DisposableResource>(resource: T): T => {
      if (disposed) {
        resource.dispose();
        throw new Error('Cannot track a cinematic resource after disposal.');
      }
      if (!tracked.has(resource)) {
        tracked.add(resource);
        resources.push(resource);
      }
      return resource;
    },
  };
}
