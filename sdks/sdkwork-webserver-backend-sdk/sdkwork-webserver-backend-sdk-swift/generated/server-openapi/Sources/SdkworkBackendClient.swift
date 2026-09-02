import Foundation
import SDKworkCommon

public class SdkworkBackendClient {
    private let httpClient: HttpClient
    public let application: ApplicationApi
    public let applicationDomain: ApplicationDomainApi
    public let certificate: CertificateApi
    public let domain: DomainApi
    public let applicationSourceVersion: ApplicationSourceVersionApi
    public let applicationDeployment: ApplicationDeploymentApi
    public let certificateDistribution: CertificateDistributionApi
    public let nginx: NginxApi
    public let server: ServerApi
    public let serverFile: ServerFileApi
    public let agent: AgentApi
    public let audit: AuditApi

    public init(baseURL: String) {
        self.httpClient = HttpClient(baseURL: baseURL)
        self.application = ApplicationApi(client: httpClient)
        self.applicationDomain = ApplicationDomainApi(client: httpClient)
        self.certificate = CertificateApi(client: httpClient)
        self.domain = DomainApi(client: httpClient)
        self.applicationSourceVersion = ApplicationSourceVersionApi(client: httpClient)
        self.applicationDeployment = ApplicationDeploymentApi(client: httpClient)
        self.certificateDistribution = CertificateDistributionApi(client: httpClient)
        self.nginx = NginxApi(client: httpClient)
        self.server = ServerApi(client: httpClient)
        self.serverFile = ServerFileApi(client: httpClient)
        self.agent = AgentApi(client: httpClient)
        self.audit = AuditApi(client: httpClient)
    }

    public init(config: SdkConfig) {
        self.httpClient = HttpClient(config: config)
        self.application = ApplicationApi(client: httpClient)
        self.applicationDomain = ApplicationDomainApi(client: httpClient)
        self.certificate = CertificateApi(client: httpClient)
        self.domain = DomainApi(client: httpClient)
        self.applicationSourceVersion = ApplicationSourceVersionApi(client: httpClient)
        self.applicationDeployment = ApplicationDeploymentApi(client: httpClient)
        self.certificateDistribution = CertificateDistributionApi(client: httpClient)
        self.nginx = NginxApi(client: httpClient)
        self.server = ServerApi(client: httpClient)
        self.serverFile = ServerFileApi(client: httpClient)
        self.agent = AgentApi(client: httpClient)
        self.audit = AuditApi(client: httpClient)
    }
    public func setAuthToken(_ token: String) -> SdkworkBackendClient {
        httpClient.setAuthToken(token)
        return self
    }

    public func setAccessToken(_ token: String) -> SdkworkBackendClient {
        httpClient.setAccessToken(token)
        return self
    }

    public func setHeader(_ key: String, value: String) -> SdkworkBackendClient {
        httpClient.setHeader(key, value: value)
        return self
    }
}
