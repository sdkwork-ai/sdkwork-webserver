import Foundation

/// API modules for sdkwork-webserver-backend-sdk
public struct API {
    public static let application = ApplicationApi.self
    public static let applicationDomain = ApplicationDomainApi.self
    public static let certificate = CertificateApi.self
    public static let domain = DomainApi.self
    public static let applicationSourceVersion = ApplicationSourceVersionApi.self
    public static let applicationDeployment = ApplicationDeploymentApi.self
    public static let certificateDistribution = CertificateDistributionApi.self
    public static let nginx = NginxApi.self
    public static let server = ServerApi.self
    public static let serverFile = ServerFileApi.self
    public static let agent = AgentApi.self
    public static let audit = AuditApi.self
}
