import { describe, expect, it, vi } from 'vitest';

import { createCinematicResourceScope } from './cinematicResources';

describe('cinematic resource scope', () => {
  it('disposes unique resources in reverse construction order exactly once', () => {
    const calls: string[] = [];
    const first = { dispose: vi.fn(() => { calls.push('first'); }) };
    const second = { dispose: vi.fn(() => { calls.push('second'); }) };
    const scope = createCinematicResourceScope();

    scope.track(first);
    scope.track(second);
    scope.track(first);
    scope.dispose();
    scope.dispose();

    expect(calls).toEqual(['second', 'first']);
    expect(first.dispose).toHaveBeenCalledOnce();
    expect(second.dispose).toHaveBeenCalledOnce();
  });
});
