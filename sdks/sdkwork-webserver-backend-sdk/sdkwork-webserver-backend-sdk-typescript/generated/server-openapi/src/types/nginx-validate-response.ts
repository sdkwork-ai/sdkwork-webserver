export interface NginxValidateResponse {
  valid?: boolean;
  errors?: ({ line?: number; message?: string; severity?: 'error' | 'warning' | 'info'; })[];
}
