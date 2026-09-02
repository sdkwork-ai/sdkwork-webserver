import 'src/http/client.dart';
import 'src/http/sdk_config.dart';
import 'src/api/application.dart';
import 'src/api/application_domain.dart';
import 'src/api/certificate.dart';
import 'src/api/domain.dart';
import 'src/api/application_source_version.dart';
import 'src/api/application_deployment.dart';
import 'src/api/certificate_distribution.dart';
import 'src/api/nginx.dart';
import 'src/api/server.dart';
import 'src/api/server_file.dart';
import 'src/api/agent.dart';
import 'src/api/audit.dart';

class SdkworkBackendClient {
  final HttpClient _httpClient;

  late final ApplicationApi application;
  late final ApplicationDomainApi applicationDomain;
  late final CertificateApi certificate;
  late final DomainApi domain;
  late final ApplicationSourceVersionApi applicationSourceVersion;
  late final ApplicationDeploymentApi applicationDeployment;
  late final CertificateDistributionApi certificateDistribution;
  late final NginxApi nginx;
  late final ServerApi server;
  late final ServerFileApi serverFile;
  late final AgentApi agent;
  late final AuditApi audit;

  SdkworkBackendClient({
    required SdkConfig config,
  }) : _httpClient = HttpClient(config: config) {
    application = ApplicationApi(_httpClient);
    applicationDomain = ApplicationDomainApi(_httpClient);
    certificate = CertificateApi(_httpClient);
    domain = DomainApi(_httpClient);
    applicationSourceVersion = ApplicationSourceVersionApi(_httpClient);
    applicationDeployment = ApplicationDeploymentApi(_httpClient);
    certificateDistribution = CertificateDistributionApi(_httpClient);
    nginx = NginxApi(_httpClient);
    server = ServerApi(_httpClient);
    serverFile = ServerFileApi(_httpClient);
    agent = AgentApi(_httpClient);
    audit = AuditApi(_httpClient);
  }

  factory SdkworkBackendClient.withBaseUrl({
    required String baseUrl,
    String? authToken,
    String? accessToken,
    Map<String, String>? headers,
    int timeout = 30000,
  }) {
    return SdkworkBackendClient(
      config: SdkConfig(
        baseUrl: baseUrl,
        timeout: timeout,
        headers: headers ?? const {},
        authToken: authToken,
        accessToken: accessToken,
      ),
    );
  }

  void setAuthToken(String token) {
    _httpClient.setAuthToken(token);
  }

  void setAccessToken(String token) {
    _httpClient.setAccessToken(token);
  }

  void setHeader(String key, String value) {
    _httpClient.setHeader(key, value);
  }

  void close() {
    _httpClient.close();
  }
}
