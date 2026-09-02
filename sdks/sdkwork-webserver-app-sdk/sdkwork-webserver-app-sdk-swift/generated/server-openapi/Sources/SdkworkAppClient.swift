import Foundation
import SDKworkCommon

public class SdkworkAppClient {
    private let httpClient: HttpClient
    public let application: ApplicationApi
    public let domain: DomainApi
    public let certificate: CertificateApi
    public let sourceVersion: SourceVersionApi
    public let deployment: DeploymentApi
    public let envVariable: EnvVariableApi
    public let monitor: MonitorApi

    public init(baseURL: String) {
        self.httpClient = HttpClient(baseURL: baseURL)
        self.application = ApplicationApi(client: httpClient)
        self.domain = DomainApi(client: httpClient)
        self.certificate = CertificateApi(client: httpClient)
        self.sourceVersion = SourceVersionApi(client: httpClient)
        self.deployment = DeploymentApi(client: httpClient)
        self.envVariable = EnvVariableApi(client: httpClient)
        self.monitor = MonitorApi(client: httpClient)
    }

    public init(config: SdkConfig) {
        self.httpClient = HttpClient(config: config)
        self.application = ApplicationApi(client: httpClient)
        self.domain = DomainApi(client: httpClient)
        self.certificate = CertificateApi(client: httpClient)
        self.sourceVersion = SourceVersionApi(client: httpClient)
        self.deployment = DeploymentApi(client: httpClient)
        self.envVariable = EnvVariableApi(client: httpClient)
        self.monitor = MonitorApi(client: httpClient)
    }
    public func setAuthToken(_ token: String) -> SdkworkAppClient {
        httpClient.setAuthToken(token)
        return self
    }

    public func setAccessToken(_ token: String) -> SdkworkAppClient {
        httpClient.setAccessToken(token)
        return self
    }

    public func setHeader(_ key: String, value: String) -> SdkworkAppClient {
        httpClient.setHeader(key, value: value)
        return self
    }
}
