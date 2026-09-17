/// Resolved runtime environment for the Flutter mobile root.
///
/// The deployment profiles materialize `env/sdkwork.<profileId>.json` as
/// `--dart-define` pairs, so every value below arrives through
/// `String.fromEnvironment` and nothing in the bundle reads a hardcoded host
/// (`SOURCE_CONFIG_SPEC.md`). An unconfigured build must fail loudly rather than
/// fall back to a default host: a silent default is how a staging build ends up
/// talking to production.
library;

import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

const String _configuredAppApiBaseUrl = String.fromEnvironment(
  'SDKWORK_WEBSERVER_APP_API_BASE_URL',
  defaultValue: '',
);

const String _configuredApplicationPublicHttpUrl = String.fromEnvironment(
  'SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL',
  defaultValue: '',
);

const String _configuredDeploymentProfile = String.fromEnvironment(
  'SDKWORK_DEPLOYMENT_PROFILE',
  defaultValue: '',
);

const String _configuredEnvironment = String.fromEnvironment(
  'SDKWORK_ENVIRONMENT',
  defaultValue: '',
);

const String _configuredProfileId = String.fromEnvironment(
  'SDKWORK_PROFILE_ID',
  defaultValue: '',
);

const String _configuredRuntimeTarget = String.fromEnvironment(
  'SDKWORK_RUNTIME_TARGET',
  defaultValue: '',
);

/// The one runtime target the Flutter mobile profiles declare.
const String webserverFlutterRuntimeTarget = 'flutter-android';

/// The environments the parent deployment index defines.
const List<String> webserverFlutterEnvironments = <String>[
  'development',
  'test',
  'staging',
  'demo',
  'production',
];

class WebserverFlutterEnvironment {
  const WebserverFlutterEnvironment({
    required this.appApiBaseUrl,
    required this.applicationPublicHttpUrl,
    required this.deploymentProfile,
    required this.environment,
    required this.profileId,
    required this.runtimeTarget,
  });

  /// Surface URL including the `/app/v3/api` prefix.
  final String appApiBaseUrl;

  /// Origin the browser/customer traffic reaches for this profile.
  final String applicationPublicHttpUrl;

  final String deploymentProfile;
  final String environment;
  final String profileId;
  final String runtimeTarget;
}

bool _isAbsoluteHttpUrl(String value) {
  final uri = Uri.tryParse(value);
  return uri != null &&
      uri.hasScheme &&
      (uri.scheme == 'http' || uri.scheme == 'https') &&
      uri.host.isNotEmpty;
}

/// Build the environment from the compile-time defines, or from explicit
/// overrides (used by tests and by the profile contract check).
WebserverFlutterEnvironment createWebserverFlutterEnvironment({
  String? appApiBaseUrl,
  String? applicationPublicHttpUrl,
  String? deploymentProfile,
  String? environment,
  String? profileId,
  String? runtimeTarget,
}) {
  final resolvedAppApiBaseUrl =
      appApiBaseUrl ?? _configuredAppApiBaseUrl;
  final resolvedPublicUrl =
      applicationPublicHttpUrl ?? _configuredApplicationPublicHttpUrl;
  final resolvedDeploymentProfile =
      deploymentProfile ?? _configuredDeploymentProfile;
  final resolvedEnvironment = environment ?? _configuredEnvironment;
  final resolvedProfileId = profileId ?? _configuredProfileId;
  final resolvedRuntimeTarget = runtimeTarget ?? _configuredRuntimeTarget;

  if (resolvedRuntimeTarget != webserverFlutterRuntimeTarget) {
    throw StateError(
      'SDKWORK_RUNTIME_TARGET must be $webserverFlutterRuntimeTarget for this '
      'root, received "$resolvedRuntimeTarget"',
    );
  }
  if (resolvedDeploymentProfile.isEmpty) {
    throw StateError('SDKWORK_DEPLOYMENT_PROFILE is not configured');
  }
  if (!webserverFlutterEnvironments.contains(resolvedEnvironment)) {
    throw StateError(
      'SDKWORK_ENVIRONMENT must be one of '
      '${webserverFlutterEnvironments.join(", ")}, received '
      '"$resolvedEnvironment"',
    );
  }
  final expectedProfileId = '$resolvedDeploymentProfile.$resolvedEnvironment';
  if (resolvedProfileId != expectedProfileId) {
    throw StateError(
      'SDKWORK_PROFILE_ID must be $expectedProfileId, received '
      '"$resolvedProfileId"',
    );
  }
  if (!_isAbsoluteHttpUrl(resolvedPublicUrl)) {
    throw StateError(
      'SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL must be an absolute '
      'HTTP(S) URL',
    );
  }

  // Throws when the surface prefix is missing or duplicated.
  final normalizedSurfaceUrl =
      normalizeWebserverAppApiBaseUrl(resolvedAppApiBaseUrl);

  return WebserverFlutterEnvironment(
    appApiBaseUrl: normalizedSurfaceUrl,
    applicationPublicHttpUrl: resolvedPublicUrl,
    deploymentProfile: resolvedDeploymentProfile,
    environment: resolvedEnvironment,
    profileId: resolvedProfileId,
    runtimeTarget: resolvedRuntimeTarget,
  );
}
