/* eslint-disable react-hooks/set-state-in-effect, react-refresh/only-export-components */
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { CLIENT_AUTH_INVALIDATED_EVENT, ClientApiError, clientRequest } from '../api/clientApi';
import { clearClientToken, getClientToken, setClientToken } from './clientSession';
import type { QrAuthentication } from './qrLoginApi';
import { authenticateWithPasskey } from './passkeyApi';

export interface ClientUser { Id: string; Name: string; Policy?: { IsDisabled?: boolean; IsAdministrator?: boolean }; }
interface ClientAuthValue { user: ClientUser | null; isLoading: boolean; signIn: (username: string, password: string) => Promise<void>; signInWithPasskey: (username?: string) => Promise<void>; adoptAuthentication: (authentication: QrAuthentication) => Promise<void>; signOut: () => Promise<void>; }
const ClientAuthContext = createContext<ClientAuthValue | null>(null);

export function ClientAuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<ClientUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  useEffect(() => {
    if (!getClientToken()) { setIsLoading(false); return; }
    void clientRequest<ClientUser>('/Users/Me').then(setUser).catch(() => { clearClientToken(); setUser(null); }).finally(() => { setIsLoading(false); });
  }, []);
  useEffect(() => {
    if (user === null) return undefined;
    let disposed = false;
    const validate = () => {
      if (disposed || document.visibilityState === 'hidden') return;
      void clientRequest<ClientUser>('/Users/Me').then((current) => {
        if (!disposed) setUser(current);
      }).catch((error: unknown) => {
        if (!disposed && error instanceof ClientApiError && error.kind === 'authentication') {
          clearClientToken();
          setUser(null);
        }
      });
    };
    const interval = window.setInterval(validate, 60_000);
    window.addEventListener('focus', validate);
    document.addEventListener('visibilitychange', validate);
    const invalidated = () => { if (!disposed) { clearClientToken(); setUser(null); } };
    window.addEventListener(CLIENT_AUTH_INVALIDATED_EVENT, invalidated);
    return () => {
      disposed = true;
      window.clearInterval(interval);
      window.removeEventListener('focus', validate);
      document.removeEventListener('visibilitychange', validate);
      window.removeEventListener(CLIENT_AUTH_INVALIDATED_EVENT, invalidated);
    };
  }, [user]);
  const value = useMemo<ClientAuthValue>(() => ({
    user,
    isLoading,
    async signIn(username, password) {
      const auth = await clientRequest<{ AccessToken?: string }>('/Users/AuthenticateByName', { method: 'POST', body: JSON.stringify({ Username: username, Pw: password }) });
      if (!auth.AccessToken) throw new ClientApiError(200, 'invalid-response');
      setClientToken(auth.AccessToken);
      try {
        const current = await clientRequest<ClientUser>('/Users/Me');
        if (current.Policy?.IsDisabled) throw new ClientApiError(403, 'authorization');
        setUser(current);
      } catch (error) { clearClientToken(); setUser(null); throw error; }
    },
    async signInWithPasskey(username) {
      const auth = await authenticateWithPasskey(username);
      if (!auth.AccessToken) throw new ClientApiError(200, 'invalid-response');
      setClientToken(auth.AccessToken);
      const current = await clientRequest<ClientUser>('/Users/Me');
      setUser(current);
    },
    adoptAuthentication(authentication) {
      if (!authentication.AccessToken) throw new ClientApiError(200, 'invalid-response');
      setClientToken(authentication.AccessToken);
      if (authentication.User.Policy?.IsDisabled) {
        clearClientToken();
        setUser(null);
        throw new ClientApiError(403, 'authorization');
      }
      setUser(authentication.User);
      return Promise.resolve();
    },
    async signOut() {
      try { await clientRequest('/Sessions/Logout', { method: 'POST' }); } finally { clearClientToken(); setUser(null); }
    },
  }), [isLoading, user]);
  return <ClientAuthContext.Provider value={value}>{children}</ClientAuthContext.Provider>;
}

export function useClientAuth(): ClientAuthValue {
  const value = useContext(ClientAuthContext);
  if (!value) throw new Error('useClientAuth must be used inside ClientAuthProvider');
  return value;
}
