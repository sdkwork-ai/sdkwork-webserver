export type WebserverDeploymentProfile = 'standalone' | 'cloud';
export type WebserverLifecycleEnvironment = 'development' | 'test' | 'staging' | 'production';

export interface BrowserRuntimeProfile {
  deploymentProfile: WebserverDeploymentProfile;
  environment: WebserverLifecycleEnvironment;
  profileId: `${WebserverDeploymentProfile}.${WebserverLifecycleEnvironment}`;
}

export interface BrowserDevelopmentServer {
  host: string;
  port: number;
  profileId: string;
  proxyTarget?: string;
}

export interface BrowserDevelopmentServerOptions {
  appRoot: string;
  deploymentProfile: WebserverDeploymentProfile;
  environment: WebserverLifecycleEnvironment;
  processEnv?: Readonly<Record<string, string | undefined>>;
  readText?: (file: string) => string;
}

export const CANONICAL_API_PROXY_PATHS: readonly string[];

export function resolveBrowserDistOutDir(
  environment: WebserverLifecycleEnvironment,
  deploymentProfile?: WebserverDeploymentProfile,
): string;

export function resolveViteRuntimeProfile(
  mode: string,
  processEnv?: Readonly<Record<string, string | undefined>>,
): BrowserRuntimeProfile;

export function resolveBrowserDevelopmentServer(
  options: BrowserDevelopmentServerOptions,
): BrowserDevelopmentServer;

export function createCanonicalApiProxyConfig(target: string): Record<string, {
  changeOrigin: false;
  target: string;
}>;
