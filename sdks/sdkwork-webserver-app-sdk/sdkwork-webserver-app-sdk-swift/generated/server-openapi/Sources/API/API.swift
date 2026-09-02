import Foundation

/// API modules for sdkwork-webserver-app-sdk
public struct API {
    public static let application = ApplicationApi.self
    public static let domain = DomainApi.self
    public static let certificate = CertificateApi.self
    public static let sourceVersion = SourceVersionApi.self
    public static let deployment = DeploymentApi.self
    public static let envVariable = EnvVariableApi.self
    public static let monitor = MonitorApi.self
}
