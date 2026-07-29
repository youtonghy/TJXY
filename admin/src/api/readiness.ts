export async function checkServerReadiness(signal: AbortSignal): Promise<boolean> {
  try {
    const response = await fetch(new Request(
      new URL('/health/ready', window.location.origin),
      { signal },
    ));
    return response.ok;
  } catch {
    return false;
  }
}
