export function reserveOAuthPopup(name: string): Window | null {
  try {
    const popup = window.open('about:blank', name, 'popup');
    if (popup === null) return null;

    // A `noopener` feature makes window.open return null even when it succeeds.
    // Reserve the window first so blocking remains detectable, then detach it.
    popup.opener = null;
    return popup;
  } catch {
    return null;
  }
}

export function navigateOAuthPopup(popup: Window, authorizationUrl: string): boolean {
  try {
    popup.location.replace(authorizationUrl);
    return true;
  } catch {
    closeOAuthPopup(popup);
    return false;
  }
}

export function closeOAuthPopup(popup: Window | null): void {
  if (popup === null) return;
  try {
    popup.close();
  } catch {
    // A provider may have navigated the popup cross-origin before cleanup.
  }
}
