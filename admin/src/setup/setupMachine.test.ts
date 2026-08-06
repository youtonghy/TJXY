import { initialSetupState, setupReducer } from './setupMachine';

it('navigates eight screens while retaining four data steps', () => {
  let state = initialSetupState();
  expect(state.screen).toBe('intro');
  state = setupReducer(state, { type: 'advance' });
  expect(state.screen).toBe('welcome');
  for (const screen of ['branding', 'database', 'network', 'administrator', 'review'] as const) {
    state = setupReducer(state, { type: 'advance' });
    expect(state.screen).toBe(screen);
  }
  expect(state.step).toBe(4);
  state = setupReducer(state, { type: 'install' });
  expect(state.screen).toBe('progress');
  expect(setupReducer(state, { type: 'back' })).toEqual(state);
  state = {
    ...state,
    databaseDrafts: {
      ...state.databaseDrafts,
      postgresql: { ...state.databaseDrafts.postgresql, Password: 'postgres-secret' },
      mysql: { ...state.databaseDrafts.mysql, Password: 'mysql-secret' },
    },
  };
  state = setupReducer(state, { type: 'completed', destinationUrl: 'http://127.0.0.1:8096/admin/login' });
  expect(state.screen).toBe('complete');
  expect(state.databaseDrafts.postgresql.Password).toBe('');
  expect(state.databaseDrafts.mysql.Password).toBe('');
});

it('retains backend drafts and invalidates only the changed test result', () => {
  let state = initialSetupState();
  state = setupReducer(state, { type: 'database-tested', backend: 'sqlite', fingerprint: 'sqlite-a' });
  state = setupReducer(state, { type: 'select-database', backend: 'postgresql' });
  state = setupReducer(state, { type: 'update-postgresql-host', host: 'db.internal' });
  state = setupReducer(state, { type: 'database-tested', backend: 'postgresql', fingerprint: 'pg-a' });
  state = setupReducer(state, { type: 'select-database', backend: 'sqlite' });
  expect(state.databaseDrafts.postgresql.Host).toBe('db.internal');
  expect(state.databaseTests.sqlite).toBe('sqlite-a');
  expect(state.databaseTests.postgresql).toBe('pg-a');
  state = setupReducer(state, { type: 'update-sqlite-path', path: '/data/other.db' });
  expect(state.databaseTests.sqlite).toBeNull();
  expect(state.databaseTests.postgresql).toBe('pg-a');
});
