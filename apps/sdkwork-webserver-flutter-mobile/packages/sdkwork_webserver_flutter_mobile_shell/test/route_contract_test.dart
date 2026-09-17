import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_webserver_flutter_mobile_shell/sdkwork_webserver_flutter_mobile_shell.dart';

/// Requires the Flutter toolchain (`flutter test`). It is committed so the
/// contract travels with the source; `tests/flutter-surface-contract.test.mjs`
/// asserts that these behaviours stay scheduled.
void main() {
  test('reports every violation of the route contract', () {
    expect(validateWebserverFlutterRouteContributions(const [_valid]), isEmpty);

    expect(
      validateWebserverFlutterRouteContributions(const [
        WebserverFlutterRouteContribution(
          id: 'app.webserver.applications.detail',
          domain: 'webserver',
          capability: 'applications',
          screen: 'list',
          routeName: '/applications',
          titleKey: 'applications.list.title',
          auth: WebserverFlutterRouteAuth.requiredAccess,
        ),
      ]),
      <String>[
        'route id app.webserver.applications.detail must equal '
            'app.webserver.applications.list',
      ],
    );

    expect(
      validateWebserverFlutterRouteContributions(const [_valid, _valid]).length,
      2,
      reason: 'a duplicate id and a duplicate route name are both violations',
    );

    expect(
      validateWebserverFlutterRouteContributions(const [
        WebserverFlutterRouteContribution(
          id: 'app.webserver.applications.list',
          domain: 'webserver',
          capability: 'applications',
          screen: 'list',
          routeName: '   ',
          titleKey: 'applications.list.title',
          auth: WebserverFlutterRouteAuth.requiredAccess,
        ),
      ]),
      <String>['route app.webserver.applications.list must declare a route name'],
    );

    expect(
      validateWebserverFlutterRouteContributions(const [
        WebserverFlutterRouteContribution(
          id: 'app.webserver.applications.list',
          domain: 'webserver',
          capability: 'applications',
          screen: 'list',
          routeName: '/applications',
          titleKey: 'applications.list.title',
          auth: WebserverFlutterRouteAuth.requiredAccess,
          navigation: WebserverFlutterNavigationContribution(
            labelKey: 'navigation.applications',
            permission: 'deploy.apps.read',
            order: 10,
          ),
        ),
      ]),
      <String>[
        'route app.webserver.applications.list contributes navigation but '
            'declares no permissionHint',
      ],
    );
  });

  test('decides route access before any page renders', () {
    expect(
      resolveWebserverFlutterRouteAccess(
        route: _valid,
        isAuthenticated: false,
        hasPermission: (_) => true,
      ).reason,
      WebserverFlutterRouteAccessReason.unauthenticated,
    );
    expect(
      resolveWebserverFlutterRouteAccess(
        route: _valid,
        isAuthenticated: true,
        hasPermission: (_) => false,
      ).reason,
      WebserverFlutterRouteAccessReason.forbidden,
    );
    expect(
      resolveWebserverFlutterRouteAccess(
        route: _valid,
        isAuthenticated: true,
        hasPermission: (permission) => permission == 'deploy.apps.read',
      ).allowed,
      isTrue,
    );

    const publicRoute = WebserverFlutterRouteContribution(
      id: 'app.webserver.health.list',
      domain: 'webserver',
      capability: 'health',
      screen: 'list',
      routeName: '/health',
      titleKey: 'health.list.title',
      auth: WebserverFlutterRouteAuth.publicAccess,
    );
    expect(
      resolveWebserverFlutterRouteAccess(
        route: publicRoute,
        isAuthenticated: false,
        hasPermission: (_) => false,
      ).allowed,
      isTrue,
      reason: 'a public route must not be blocked by a missing permission',
    );
  });

  test('mounts only named routes and picks the lowest navigation order as home', () {
    expect(createWebserverFlutterRouteRegistry(const [_valid]).length, 1);
    expect(
      createWebserverFlutterRouteRegistry(const [
        WebserverFlutterRouteContribution(
          id: 'app.webserver.applications.list',
          domain: 'webserver',
          capability: 'applications',
          screen: 'list',
          routeName: '   ',
          titleKey: 'applications.list.title',
          auth: WebserverFlutterRouteAuth.requiredAccess,
        ),
      ]),
      isEmpty,
      reason: 'an unnamed route would be mounted at an empty path and swallow '
          'every other route',
    );
    expect(resolveWebserverFlutterHomeRouteName(const [_valid]), '/applications');
    expect(resolveWebserverFlutterHomeRouteName(const []), isNull);
  });
}

const WebserverFlutterRouteContribution _valid =
    WebserverFlutterRouteContribution(
  id: 'app.webserver.applications.list',
  domain: 'webserver',
  capability: 'applications',
  screen: 'list',
  routeName: '/applications',
  titleKey: 'applications.list.title',
  auth: WebserverFlutterRouteAuth.requiredAccess,
  permissionHint: 'deploy.apps.read',
  navigation: WebserverFlutterNavigationContribution(
    labelKey: 'navigation.applications',
    permission: 'deploy.apps.read',
    order: 10,
  ),
);
