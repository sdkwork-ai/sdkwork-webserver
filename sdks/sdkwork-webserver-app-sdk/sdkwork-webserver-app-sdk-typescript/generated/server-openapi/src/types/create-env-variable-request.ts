export interface CreateEnvVariableRequest {
  key: string;
  value: string;
  environment?: 'development' | 'test' | 'staging' | 'production';
  isSecret?: boolean;
}
