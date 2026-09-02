import 'src/http/client.dart';
import 'src/http/sdk_config.dart';
import 'src/api/application.dart';
import 'src/api/domain.dart';
import 'src/api/certificate.dart';
import 'src/api/source_version.dart';
import 'src/api/deployment.dart';
import 'src/api/env_variable.dart';
import 'src/api/monitor.dart';

class SdkworkAppClient {
  final HttpClient _httpClient;

  late final ApplicationApi application;
  late final DomainApi domain;
  late final CertificateApi certificate;
  late final SourceVersionApi sourceVersion;
  late final DeploymentApi deployment;
  late final EnvVariableApi envVariable;
  late final MonitorApi monitor;

  SdkworkAppClient({
    required SdkConfig config,
  }) : _httpClient = HttpClient(config: config) {
    application = ApplicationApi(_httpClient);
    domain = DomainApi(_httpClient);
    certificate = CertificateApi(_httpClient);
    sourceVersion = SourceVersionApi(_httpClient);
    deployment = DeploymentApi(_httpClient);
    envVariable = EnvVariableApi(_httpClient);
    monitor = MonitorApi(_httpClient);
  }

  factory SdkworkAppClient.withBaseUrl({
    required String baseUrl,
    String? authToken,
    String? accessToken,
    Map<String, String>? headers,
    int timeout = 30000,
  }) {
    return SdkworkAppClient(
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
