const FALLBACK_DESTINATION = '/admin/users';
const BLOCKED_DESTINATIONS = [
  '/admin/login',
  '/admin/authentication-error',
  '/admin/access-denied',
] as const;

export function loginDestination(state: unknown, origin: string): string {
  if (!isRecord(state) || typeof state.nextPathname !== 'string') {
    return FALLBACK_DESTINATION;
  }

  const search = state.nextSearch;
  if (
    (search !== undefined && (typeof search !== 'string' || (search !== '' && !search.startsWith('?'))))
    || containsControlCharacter(state.nextPathname)
    || (typeof search === 'string' && containsControlCharacter(search))
    || state.nextPathname.includes('?')
  ) {
    return FALLBACK_DESTINATION;
  }

  try {
    const pathnameWithoutFragment = state.nextPathname.split('#', 1)[0] ?? '';
    const destination = new URL(`${pathnameWithoutFragment}${search ?? ''}`, origin);
    if (destination.origin !== origin || !destination.pathname.startsWith('/admin/')) {
      return FALLBACK_DESTINATION;
    }
    if (BLOCKED_DESTINATIONS.some((blocked) => (
      destination.pathname === blocked || destination.pathname.startsWith(`${blocked}/`)
    ))) {
      return FALLBACK_DESTINATION;
    }
    return `${destination.pathname}${destination.search}`;
  } catch {
    return FALLBACK_DESTINATION;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function containsControlCharacter(value: string): boolean {
  for (const character of value) {
    const code = character.charCodeAt(0);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}
