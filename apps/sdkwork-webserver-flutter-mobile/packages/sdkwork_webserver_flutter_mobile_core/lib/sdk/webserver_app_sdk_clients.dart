import 'package:sdkwork_webserver_app_sdk/sdkwork_webserver_app_sdk.dart';

/// The v3 app-api surface every SDKWork client root addresses.
///
/// The generated Dart client already prepends this prefix itself
/// (`ApiPaths.apiPrefix`), so `SdkworkAppClient` must be constructed with the
/// **stripped** origin — see [resolveWebserverAppTransportBaseUrl]. Passing the
/// prefixed URL to the client would produce `/app/v3/api/app/v3/api/...`.
const String webserverAppApiPrefix = '/app/v3/api';

/// One generated app client plus the two URLs it was derived from.
class WebserverAppSdkClients {
  const WebserverAppSdkClients({
    required this.appApiBaseUrl,
    required this.transportBaseUrl,
    required this.webserver,
  });

  /// The validated surface URL, prefix included. This is the value the runtime
  /// profile carries and the value diagnostics should print.
  final String appApiBaseUrl;

  /// The origin actually handed to the generated client (prefix removed).
  final String transportBaseUrl;

  /// The generated Web Server app client. `sdkwork-webserver` is the only app
  /// SDK in this workspace that ships a Flutter variant, so this client is
  /// constructed from a real generated artifact rather than an adapter seam.
  final SdkworkAppClient webserver;

  void dispose() {}
}

/// Construct the generated app clients from one runtime-profile surface URL.
WebserverAppSdkClients createWebserverAppSdkClients({
  required String appApiBaseUrl,
  String? authToken,
  String? accessToken,
  int timeout = 30000,
}) {
  final normalizedSurfaceUrl = normalizeWebserverAppApiBaseUrl(appApiBaseUrl);
  final transportBaseUrl = resolveWebserverAppTransportBaseUrl(
    normalizedSurfaceUrl,
  );
  return WebserverAppSdkClients(
    appApiBaseUrl: normalizedSurfaceUrl,
    transportBaseUrl: transportBaseUrl,
    webserver: SdkworkAppClient.withBaseUrl(
      baseUrl: transportBaseUrl,
      authToken: authToken,
      accessToken: accessToken,
      timeout: timeout,
    ),
  );
}

/// Assert one absolute HTTP(S) URL that ends with [webserverAppApiPrefix].
///
/// The prefix must appear exactly once: a profile that accidentally pastes the
/// surface twice used to travel all the way into a request path before failing
/// as a 404, so the shape is rejected at construction instead.
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

/// Strip the surface prefix to obtain the origin the generated client wants.
String resolveWebserverAppTransportBaseUrl(String appApiBaseUrl) {
  final normalized = normalizeWebserverAppApiBaseUrl(appApiBaseUrl);
  final uri = Uri.parse(normalized);
  final transportPath = uri.path.substring(
    0,
    uri.path.length - webserverAppApiPrefix.length,
  );
  return uri
      .replace(path: transportPath)
      .toString()
      .replaceFirst(RegExp(r'/+$'), '');
}
