export interface UserPolicy {
  IsAdministrator: boolean;
  IsDisabled: boolean;
  EnableMediaPlayback: boolean;
  EnableAudioPlaybackTranscoding: boolean;
  EnableVideoPlaybackTranscoding: boolean;
  EnablePlaybackRemuxing: boolean;
  AuthenticationProviderId: string;
  PasswordResetProviderId: string;
}

export interface TjxyUser {
  Name: string;
  ServerId: string;
  Id: string;
  HasPassword: boolean;
  HasConfiguredPassword: boolean;
  Configuration: Record<string, never>;
  Policy: UserPolicy;
}

export interface UserRecord extends TjxyUser {
  id: string;
}

export interface SessionInfo {
  Id: string;
  UserId: string;
  UserName: string;
  Client: string;
  DeviceId: string;
  DeviceName: string;
  ApplicationVersion: string;
  ServerId: string;
  IsActive: boolean;
  PlayableMediaTypes: string[];
  SupportedCommands: string[];
}

export interface AuthenticationResult {
  User: TjxyUser;
  SessionInfo: SessionInfo;
  AccessToken: string;
  ServerId: string;
}
