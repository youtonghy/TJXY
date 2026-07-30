export function safeClientDestination(value: unknown): string {
  if (typeof value !== 'string' || !value.startsWith('/app')) return '/app/';
  if (value.startsWith('//') || value.includes('\\') || value.includes('\n')) return '/app/';
  return value;
}
