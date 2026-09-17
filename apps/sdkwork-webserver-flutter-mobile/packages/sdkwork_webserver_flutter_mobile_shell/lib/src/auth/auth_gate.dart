/// AuthGate integration for the Flutter mobile shell.
///
/// Route guards are shell/runtime responsibilities. Capability packages declare
/// auth mode and permission hints only, and never evaluate a token themselves.
library;

import '../navigation/route_registry.dart';

enum WebserverFlutterRouteAccessReason { unauthenticated, forbidden }

class WebserverFlutterRouteAccessDecision {
  const WebserverFlutterRouteAccessDecision({
    required this.allowed,
    this.reason,
  });

  final bool allowed;
  final WebserverFlutterRouteAccessReason? reason;
}

/// Decide whether a route may render *before* any page builds.
///
/// The permission hint is checked against the caller-supplied predicate so the
/// shell never imports a token store or an IAM client.
WebserverFlutterRouteAccessDecision resolveWebserverFlutterRouteAccess({
  required WebserverFlutterRouteContribution route,
  required bool isAuthenticated,
  required bool Function(String permission) hasPermission,
}) {
  if (route.auth == WebserverFlutterRouteAuth.requiredAccess &&
      !isAuthenticated) {
    return const WebserverFlutterRouteAccessDecision(
      allowed: false,
      reason: WebserverFlutterRouteAccessReason.unauthenticated,
    );
  }
  final hint = route.permissionHint;
  if (hint != null && !hasPermission(hint)) {
    return const WebserverFlutterRouteAccessDecision(
      allowed: false,
      reason: WebserverFlutterRouteAccessReason.forbidden,
    );
  }
  return const WebserverFlutterRouteAccessDecision(allowed: true);
}
