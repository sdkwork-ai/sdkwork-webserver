export interface ServerProjectOperation {
  id: string;
  kind: 'build' | 'package' | 'start' | 'deploy' | 'stop' | 'restart';
  label: string;
  /** IAM permission required to invoke the operation. */
  permission: string;
  description?: string;
  dangerous?: boolean;
}
