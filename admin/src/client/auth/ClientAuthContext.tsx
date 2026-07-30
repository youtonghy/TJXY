/* eslint-disable react-hooks/set-state-in-effect, react-refresh/only-export-components */
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { ClientApiError, clientRequest } from '../api/clientApi';
import { clearClientToken, getClientToken, setClientToken } from './clientSession';

export interface ClientUser { Id: string; Name: string; Policy?: { IsDisabled?: boolean; IsAdministrator?: boolean }; }
interface ClientAuthValue { user: ClientUser | null; isLoading: boolean; signIn: (username: string, password: string) => Promise<void>; signOut: () => Promise<void>; }
const ClientAuthContext = createContext<ClientAuthValue | null>(null);

export function ClientAuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<ClientUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  useEffect(() => {
    if (!getClientToken()) { setIsLoading(false); return; }
    void clientRequest<ClientUser>('/Users/Me').then(setUser).catch(() => { clearClientToken(); setUser(null); }).finally(() => { setIsLoading(false); });
  }, []);
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
