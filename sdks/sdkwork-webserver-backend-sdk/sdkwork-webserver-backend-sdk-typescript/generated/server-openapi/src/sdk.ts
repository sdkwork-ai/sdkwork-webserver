import { HttpClient, createHttpClient } from './http/client';
import type { SdkworkBackendConfig } from './types/common';
import type { AuthTokenManager } from '@sdkwork/sdk-common';

import { ApplicationApi, createApplicationApi } from './api/application';
import { ApplicationDomainApi, createApplicationDomainApi } from './api/application-domain';
import { CertificateApi, createCertificateApi } from './api/certificate';
import { DomainApi, createDomainApi } from './api/domain';
import { ApplicationSourceVersionApi, createApplicationSourceVersionApi } from './api/application-source-version';
import { ApplicationDeploymentApi, createApplicationDeploymentApi } from './api/application-deployment';
import { CertificateDistributionApi, createCertificateDistributionApi } from './api/certificate-distribution';
import { NginxApi, createNginxApi } from './api/nginx';
import { ServerApi, createServerApi } from './api/server';
import { ServerFileApi, createServerFileApi } from './api/server-file';
import { WebserverConfigApi, createWebserverConfigApi } from './api/webserver-config';
import { AgentApi, createAgentApi } from './api/agent';
import { AuditApi, createAuditApi } from './api/audit';
import { ClusterApi, createClusterApi } from './api/cluster';
import { TrafficUsageApi, createTrafficUsageApi } from './api/traffic-usage';
import { MetricsSummaryApi, createMetricsSummaryApi } from './api/metrics-summary';

export class SdkworkBackendClient {
  private httpClient: HttpClient;

  public readonly application: ApplicationApi;
  public readonly applicationDomain: ApplicationDomainApi;
  public readonly certificate: CertificateApi;
  public readonly domain: DomainApi;
  public readonly applicationSourceVersion: ApplicationSourceVersionApi;
  public readonly applicationDeployment: ApplicationDeploymentApi;
  public readonly certificateDistribution: CertificateDistributionApi;
  public readonly nginx: NginxApi;
  public readonly server: ServerApi;
  public readonly serverFile: ServerFileApi;
  public readonly webserverConfig: WebserverConfigApi;
  public readonly agent: AgentApi;
  public readonly audit: AuditApi;
  public readonly cluster: ClusterApi;
  public readonly trafficUsage: TrafficUsageApi;
  public readonly metricsSummary: MetricsSummaryApi;

  constructor(config: SdkworkBackendConfig) {
    this.httpClient = createHttpClient(config);
    this.application = createApplicationApi(this.httpClient);

    this.applicationDomain = createApplicationDomainApi(this.httpClient);

    this.certificate = createCertificateApi(this.httpClient);

    this.domain = createDomainApi(this.httpClient);

    this.applicationSourceVersion = createApplicationSourceVersionApi(this.httpClient);

    this.applicationDeployment = createApplicationDeploymentApi(this.httpClient);

    this.certificateDistribution = createCertificateDistributionApi(this.httpClient);

    this.nginx = createNginxApi(this.httpClient);

    this.server = createServerApi(this.httpClient);

    this.serverFile = createServerFileApi(this.httpClient);

    this.webserverConfig = createWebserverConfigApi(this.httpClient);

    this.agent = createAgentApi(this.httpClient);

    this.audit = createAuditApi(this.httpClient);

    this.cluster = createClusterApi(this.httpClient);

    this.trafficUsage = createTrafficUsageApi(this.httpClient);

    this.metricsSummary = createMetricsSummaryApi(this.httpClient);
  }
  setAuthToken(token: string): this {
    this.httpClient.setAuthToken(token);
    return this;
  }

  setAccessToken(token: string): this {
    this.httpClient.setAccessToken(token);
    return this;
  }

  setTokenManager(manager: AuthTokenManager): this {
    this.httpClient.setTokenManager(manager);
    return this;
  }

  get http(): HttpClient {
    return this.httpClient;
  }
}

export function createClient(config: SdkworkBackendConfig): SdkworkBackendClient {
  return new SdkworkBackendClient(config);
}

export default SdkworkBackendClient;
