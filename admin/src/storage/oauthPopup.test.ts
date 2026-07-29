import { closeOAuthPopup, navigateOAuthPopup, reserveOAuthPopup } from './oauthPopup';

let replaceMock = vi.fn();
let closeMock = vi.fn();

function fakePopup(): Window {
  return {
    opener: window,
    location: { replace: replaceMock },
    close: closeMock,
  } as unknown as Window;
}

beforeEach(() => {
  replaceMock = vi.fn();
  closeMock = vi.fn();
});

afterEach(() => { vi.restoreAllMocks(); });

it('reserves a detectable popup, detaches its opener, and then navigates it', () => {
  const popup = fakePopup();
  const openMock = vi.spyOn(window, 'open').mockReturnValue(popup);

  const reserved = reserveOAuthPopup('tjxy-google-oauth');

  expect(openMock).toHaveBeenCalledWith('about:blank', 'tjxy-google-oauth', 'popup');
  expect(reserved).toBe(popup);
  expect(popup.opener).toBeNull();
  expect(navigateOAuthPopup(popup, 'https://accounts.google.com/auth')).toBe(true);
  expect(replaceMock).toHaveBeenCalledWith('https://accounts.google.com/auth');
});

it('distinguishes a blocked popup and closes a failed navigation', () => {
  vi.spyOn(window, 'open').mockReturnValue(null);
  expect(reserveOAuthPopup('tjxy-onedrive-oauth')).toBeNull();

  const popup = fakePopup();
  replaceMock.mockImplementation(() => { throw new DOMException('blocked'); });
  expect(navigateOAuthPopup(popup, 'https://login.microsoftonline.com/auth')).toBe(false);
  expect(closeMock).toHaveBeenCalledOnce();

  closeOAuthPopup(popup);
  expect(closeMock).toHaveBeenCalledTimes(2);
});
