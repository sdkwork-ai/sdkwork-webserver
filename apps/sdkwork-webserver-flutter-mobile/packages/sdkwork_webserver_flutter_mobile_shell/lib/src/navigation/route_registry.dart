/// Flutter route placement metadata and projection inputs.
///
/// Authority: `FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` and
/// `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` section 7. Route ids follow
/// `<surface>.<domain>.<capability>.<screen>` and stay aligned with the PC, H5,
/// mini program, and HarmonyOS roots; the physical Flutter route name may
/// differ.
///
/// Route metadata must not declare HTTP API paths, SDK methods, raw URL
/// constants, or transport details.
library;

enum WebserverFlutterRouteAuth { publicAccess, requiredAccess }

/// Navigation slot a route occupies, and its idle position in the tab bar.
class WebserverFlutterNavigationContribution {
  const WebserverFlutterNavigationContribution({
    required this.labelKey,
    required this.permission,
    required this.order,
  });

  final String labelKey;
  final String permission;
  final int order;
}

class WebserverFlutterRouteContribution {
  const WebserverFlutterRouteContribution({
    required this.id,
    required this.domain,
    required this.capability,
    required this.screen,
    required this.routeName,
    required this.titleKey,
    required this.auth,
    this.surface = 'app',
    this.permissionHint,
    this.navigation,
  });

  final String id;
  final String surface;
  final String domain;
  final String capability;
  final String screen;

  /// Physical Flutter route name, e.g. `/applications`.
  final String routeName;

  final String titleKey;
  final WebserverFlutterRouteAuth auth;

  /// Permission the owning module derives from the route's own operationId.
  final String? permissionHint;

  final WebserverFlutterNavigationContribution? navigation;
}

/// Validate the canonical route id shape, uniqueness, and the
/// placement/identity agreement.
///
/// Returns issues rather than throwing so verification can report every
/// violation in one run.
List<String> validateWebserverFlutterRouteContributions(
  List<WebserverFlutterRouteContribution> routes,
) {
  final issues = <String>[];
  final seenIds = <String>[];
  final seenRouteNames = <String>[];
  for (final route in routes) {
    final expectedId =
        '${route.surface}.${route.domain}.${route.capability}.${route.screen}';
    if (route.id != expectedId) {
      issues.add('route id ${route.id} must equal $expectedId');
    }
    if (seenIds.contains(route.id)) {
      issues.add('duplicate flutter route id ${route.id}');
    }
    seenIds.add(route.id);
    if (route.routeName.trim().isEmpty) {
      issues.add('route ${route.id} must declare a route name');
    }
    if (seenRouteNames.contains(route.routeName)) {
      issues.add('duplicate flutter route name ${route.routeName}');
    }
    seenRouteNames.add(route.routeName);
    if (route.navigation != null && route.permissionHint == null) {
      issues.add(
        'route ${route.id} contributes navigation but declares no permissionHint',
      );
    }
  }
  return issues;
}

/// Assemble the router table from capability contributions.
///
/// Contributions with no route name are dropped rather than mounted at an empty
/// path, which would swallow every other route.
List<WebserverFlutterRouteContribution> createWebserverFlutterRouteRegistry(
  List<WebserverFlutterRouteContribution> routes,
) {
  return routes
      .where((route) => route.routeName.trim().isNotEmpty)
      .toList(growable: false);
}

/// First route the shell mounts, preferring the lowest navigation order.
String? resolveWebserverFlutterHomeRouteName(
  List<WebserverFlutterRouteContribution> routes,
) {
  final navigable =
      routes.where((route) => route.navigation != null).toList(growable: false)
        ..sort((a, b) => a.navigation!.order.compareTo(b.navigation!.order));
  if (navigable.isNotEmpty) {
    return navigable.first.routeName;
  }
  return routes.isEmpty ? null : routes.first.routeName;
}
