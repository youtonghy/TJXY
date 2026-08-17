export type DatabaseBackend = 'sqlite' | 'postgresql' | 'mysql';
export type DatabaseTlsMode = 'disable' | 'prefer' | 'require';

export type DatabaseDraft =
  | { Backend: 'sqlite'; Path: string }
  | {
    Backend: 'postgresql' | 'mysql';
    Host: string;
    Port: number;
    Database: string;
    Username: string;
    Password: string;
    Tls: DatabaseTlsMode;
  };

export interface SetupStatus {
  state: 'unconfigured' | 'pending';
  installationId: string;
  csrfToken: string;
  databaseBackends: DatabaseBackend[];
  deploymentMode: 'native' | 'container';
  version: string;
  configurationWritable: boolean;
  sourceEligible: boolean;
  blockingOverrides: string[];
  managedDatabaseBackend: DatabaseBackend | null;
}

export interface DatabaseTestResult {
  backend: DatabaseBackend;
  version: string;
  elapsedMilliseconds: number;
}

export interface NetworkDraft {
  listenHost: string;
  port: number;
  publicUrl: string | null;
}

export interface NetworkValidationResult extends NetworkDraft {
  destinationUrl: string;
}

export type SetupProgressStage =
  | 'connecting_database'
  | 'migrating_database'
  | 'creating_administrator'
  | 'saving_settings'
  | 'completing_installation'
  | 'complete'
  | 'failed';

export interface SetupProgressEvent {
  installationId: string;
  stage: SetupProgressStage;
}

export interface CompleteSetupDraft {
  siteTitle: string;
  siteSubtitle: string;
  locale: 'zh-CN' | 'en-US';
  logoUrl: string;
  iconUrl: string;
  database: DatabaseDraft | null;
  network: NetworkDraft;
  administratorUsername: string;
  administratorPassword: string;
}
