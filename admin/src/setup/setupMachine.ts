import type { DatabaseBackend } from './setupTypes';

export type SetupScreen =
  | 'intro'
  | 'welcome'
  | 'branding'
  | 'database'
  | 'network'
  | 'administrator'
  | 'review'
  | 'recovery'
  | 'progress'
  | 'complete';

export interface SetupMachineState {
  screen: SetupScreen;
  step: 0 | 1 | 2 | 3 | 4;
  selectedDatabase: DatabaseBackend;
  databaseDrafts: {
    sqlite: { Backend: 'sqlite'; Path: string };
    postgresql: { Backend: 'postgresql'; Host: string; Port: number; Database: string; Username: string; Password: string; Tls: 'disable' | 'prefer' | 'require' };
    mysql: { Backend: 'mysql'; Host: string; Port: number; Database: string; Username: string; Password: string; Tls: 'disable' | 'prefer' | 'require' };
  };
  databaseTests: Record<DatabaseBackend, string | null>;
  destinationUrl: string | null;
  managedDatabaseBackend: DatabaseBackend | null;
}

export type SetupAction =
  | { type: 'advance' }
  | { type: 'back' }
  | { type: 'install' }
  | { type: 'recover' }
  | { type: 'completed'; destinationUrl: string }
  | { type: 'select-database'; backend: DatabaseBackend }
  | { type: 'database-tested'; backend: DatabaseBackend; fingerprint: string }
  | { type: 'update-sqlite-path'; path: string }
  | { type: 'update-postgresql-host'; host: string }
  | { type: 'update-database-draft'; backend: DatabaseBackend; draft: SetupMachineState['databaseDrafts'][DatabaseBackend] }
  | { type: 'set-managed-database'; backend: DatabaseBackend };

const screens: SetupScreen[] = [
  'intro', 'welcome', 'branding', 'database', 'network', 'administrator', 'review',
];

export function initialSetupState(): SetupMachineState {
  return {
    screen: 'intro',
    step: 0,
    selectedDatabase: 'sqlite',
    databaseDrafts: {
      sqlite: { Backend: 'sqlite', Path: './data/tjxy.db' },
      postgresql: { Backend: 'postgresql', Host: '127.0.0.1', Port: 5432, Database: 'tjxy', Username: 'tjxy', Password: '', Tls: 'prefer' },
      mysql: { Backend: 'mysql', Host: '127.0.0.1', Port: 3306, Database: 'tjxy', Username: 'tjxy', Password: '', Tls: 'prefer' },
    },
    databaseTests: { sqlite: null, postgresql: null, mysql: null },
    destinationUrl: null,
    managedDatabaseBackend: null,
  };
}

export function setupReducer(state: SetupMachineState, action: SetupAction): SetupMachineState {
  switch (action.type) {
    case 'advance': {
      const index = screens.indexOf(state.screen);
      if (index < 0 || index === screens.length - 1) return state;
      const nextIndex = state.screen === 'branding' && state.managedDatabaseBackend !== null
        ? index + 2
        : index + 1;
      const screen = screens[nextIndex] ?? state.screen;
      return { ...state, screen, step: stepFor(screen) };
    }
    case 'back': {
      const index = screens.indexOf(state.screen);
      if (index <= 0) return state;
      const previousIndex = state.screen === 'network' && state.managedDatabaseBackend !== null
        ? index - 2
        : index - 1;
      const screen = screens[previousIndex] ?? state.screen;
      return { ...state, screen, step: stepFor(screen) };
    }
    case 'install':
      return state.screen === 'review' ? { ...state, screen: 'progress' } : state;
    case 'recover':
      return { ...state, screen: 'recovery', step: 0 };
    case 'completed':
      return state.screen === 'progress' || state.screen === 'recovery'
        ? {
            ...state,
            screen: 'complete',
            destinationUrl: action.destinationUrl,
            databaseDrafts: {
              ...state.databaseDrafts,
              postgresql: { ...state.databaseDrafts.postgresql, Password: '' },
              mysql: { ...state.databaseDrafts.mysql, Password: '' },
            },
          }
        : state;
    case 'select-database':
      return { ...state, selectedDatabase: action.backend };
    case 'database-tested':
      return { ...state, databaseTests: { ...state.databaseTests, [action.backend]: action.fingerprint } };
    case 'update-sqlite-path':
      return {
        ...state,
        databaseDrafts: { ...state.databaseDrafts, sqlite: { Backend: 'sqlite', Path: action.path } },
        databaseTests: { ...state.databaseTests, sqlite: null },
      };
    case 'update-postgresql-host':
      return {
        ...state,
        databaseDrafts: { ...state.databaseDrafts, postgresql: { ...state.databaseDrafts.postgresql, Host: action.host } },
        databaseTests: { ...state.databaseTests, postgresql: null },
      };
    case 'update-database-draft':
      return {
        ...state,
        databaseDrafts: { ...state.databaseDrafts, [action.backend]: action.draft },
        databaseTests: { ...state.databaseTests, [action.backend]: null },
      };
    case 'set-managed-database':
      return { ...state, selectedDatabase: action.backend, managedDatabaseBackend: action.backend };
  }
}

function stepFor(screen: SetupScreen): 0 | 1 | 2 | 3 | 4 {
  if (screen === 'branding') return 1;
  if (screen === 'database') return 2;
  if (screen === 'network') return 3;
  if (screen === 'administrator' || screen === 'review') return 4;
  return 0;
}
