/// The v3 app-api surface URL contract for this root.
///
/// The deployment profile materializes a **prefixed** surface URL
/// (`SDKWORK_WEBSERVER_APP_API_BASE_URL`), so the prefix must be present exactly
/// once. A profile that accidentally pastes the surface twice used to travel all
/// the way into a request path before failing as a 404, so the shape is rejected
/// while the runtime environment is built instead.
///
/// This contract is surface-URL validation only. It is deliberately independent
/// of any generated client: the `deploy_app` entity is owned by
/// `sdkwork-deployments`, and this root reaches it through the catalog port in
/// `sdk/webserver_deploy_app_catalog_port.dart`
/// (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
library;

/// The v3 app-api surface prefix every SDKWork app API base URL carries.
const String webserverAppApiPrefix = '/app/v3/api';

/// Assert and normalize one absolute HTTP(S) URL that ends with
/// [webserverAppApiPrefix].
String normalizeWebserverAppApiBaseUrl(String value) {
  final normalized = value.trim().replaceFirst(RegExp(r'/+$'), '');
  final uri = Uri.tryParse(normalized);
  if (uri == null ||
      !uri.hasScheme ||
      (uri.scheme != 'http' && uri.scheme != 'https') ||
      uri.host.isEmpty ||
      uri.hasQuery ||
      uri.hasFragment ||
      !uri.path.endsWith(webserverAppApiPrefix)) {
    throw ArgumentError.value(
      value,
      'appApiBaseUrl',
      'must be an absolute HTTP(S) URL ending with $webserverAppApiPrefix',
    );
  }
  final prefixStart = uri.path.length - webserverAppApiPrefix.length;
  if (prefixStart > 0 &&
      uri.path
          .substring(0, prefixStart)
          .endsWith(webserverAppApiPrefix)) {
    throw ArgumentError.value(
      value,
      'appApiBaseUrl',
      'must contain $webserverAppApiPrefix exactly once',
    );
  }
  return normalized;
}
