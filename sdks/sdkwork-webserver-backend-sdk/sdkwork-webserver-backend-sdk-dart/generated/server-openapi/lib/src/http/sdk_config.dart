class SdkConfig {
  final String baseUrl;
  final int timeout;
  final Map<String, String> headers;
  final String? authToken;
  final String? accessToken;

  const SdkConfig({
    required this.baseUrl,
    this.timeout = 30000,
    this.headers = const {},
    this.authToken,
    this.accessToken,
  });
}
