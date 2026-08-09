export function safeClientDestination(value: unknown): string {
  if (typeof value !== 'string') return '/app/';
  if (value.startsWith('//') || value.includes('\\') || containsControlCharacter(value)) return '/app/';
  if (value === '/app' || value.startsWith('/app/')) return value;
  if (value === '/admin' || value.startsWith('/admin/')) {
    const pathname = value.split(/[?#]/u, 1)[0];
    if (
      pathname === '/admin/login'
      || pathname === '/admin/authentication-error'
      || pathname === '/admin/access-denied'
    ) return '/admin';
    return value;
  }
  return '/app/';
}

function containsControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.charCodeAt(0);
    return code <= 0x1f || code === 0x7f;
  });
}
