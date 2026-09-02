import type { Platform } from './platform';
import type { TechStack } from './tech-stack';

export interface PlatformTargetResponse {
  id?: string;
  appId?: string;
  targetKey?: string;
  platform?: Platform;
  techStack?: TechStack;
  architectures?: string[];
  bundleId?: string;
  packageName?: string;
  appIdValue?: string;
  bundleName?: string;
  targetStatus?: string;
  createdAt?: string;
  updatedAt?: string;
}
