import type { Platform } from './platform';
import type { TechStack } from './tech-stack';

export interface CreatePlatformTargetRequest {
  targetKey: string;
  platform: Platform;
  techStack?: TechStack;
  architectures?: string[];
  bundleId?: string;
  packageName?: string;
  appId?: string;
  bundleName?: string;
  allowedChannels?: ('stable' | 'beta' | 'alpha' | 'qa')[];
}
