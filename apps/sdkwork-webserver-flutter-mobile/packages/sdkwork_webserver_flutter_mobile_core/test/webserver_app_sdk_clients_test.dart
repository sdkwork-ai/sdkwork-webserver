import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_webserver_app_sdk/sdkwork_webserver_app_sdk.dart';
import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

/// Requires the Flutter toolchain (`flutter test`). It is committed so the
/// contract travels with the source; `tests/flutter-surface-contract.test.mjs`
/// covers everything that is verifiable without that toolchain.
void main() {
  group('webserver app SDK clients', () {
    test('constructs the generated webserver app client from one surface URL', () {
      final clients = createWebserverAppSdkClients(
        appApiBaseUrl: 'https://console.example.com/app/v3/api/',
      );

      expect(clients.appApiBaseUrl, 'https://console.example.com/app/v3/api');
      expect(clients.transportBaseUrl, 'https://console.example.com');
      expect(clients.webserver, isA<SdkworkAppClient>());
    });

    test('exposes the applications list operation the catalog depends on', () {
      final client = createWebserverAppSdkClients(
        appApiBaseUrl: 'https://console.example.com/app/v3/api',
      ).webserver;

      final Future<ApplicationsListResponse?> Function([
        int?,
        int?,
        int?,
        String?,
        int?,
        String?,
      ])
      list = client.application.applicationsList;

      expect(list, isNotNull);
    });

    test('rejects a bare origin and a duplicated surface prefix', () {
      expect(
        () => createWebserverAppSdkClients(
          appApiBaseUrl: 'https://console.example.com',
        ),
        throwsArgumentError,
      );
      expect(
        () => createWebserverAppSdkClients(
          appApiBaseUrl:
              'https://console.example.com/app/v3/api/app/v3/api',
        ),
        throwsArgumentError,
      );
      expect(
        () => createWebserverAppSdkClients(appApiBaseUrl: '   '),
        throwsArgumentError,
      );
    });
  });

  group('deploy_app catalog port', () {
    test('reports unavailable and refuses to look like an empty tenant', () {
      final port = WebserverDeployAppCatalogPort();

      expect(port.available, isFalse);
      expect(WebserverDeployAppCatalogPort.platform, 'flutter-android');
      expect(
        () => port.reader,
        throwsA(
          isA<WebserverDeployAppCatalogUnavailableError>().having(
            (error) => error.toString(),
            'toString',
            contains(WebserverDeployAppCatalogUnavailableError.code),
          ),
        ),
      );
    });

    test('bounds the closed sets to the generated deployments unions', () {
      expect(webserverDeployAppKinds, hasLength(8));
      expect(webserverDeployAppStatuses, hasLength(6));
      expect(webserverDeployAppKinds, contains('HARMONYOS_APP'));
      expect(webserverDeployAppStatuses, contains('ARCHIVED'));
    });
  });

  group('list pagination', () {
    test('narrows server page info without inventing totals', () {
      final narrowed = toWebserverFlutterListPage(
        const WebserverFlutterPageInfoLike(
          page: 1,
          pageSize: 20,
          totalItems: '45',
          totalPages: 3,
        ),
      );

      expect(narrowed.page, 1);
      expect(narrowed.pageSize, 20);
      expect(narrowed.totalItems, 45);
      expect(narrowed.totalPages, 3);
      expect(narrowed.hasMore, isTrue);
    });

    test('stops paging on the last server page', () {
      final narrowed = toWebserverFlutterListPage(
        const WebserverFlutterPageInfoLike(
          page: 3,
          pageSize: 20,
          totalPages: 3,
          hasMore: false,
        ),
      );

      expect(narrowed.hasMore, isFalse);
    });

    test('applies defaults when the server omits page info', () {
      final narrowed = toWebserverFlutterListPage(null);

      expect(narrowed.page, 1);
      expect(narrowed.pageSize, defaultWebserverFlutterListPageSize);
      expect(narrowed.totalItems, 0);
      expect(narrowed.totalPages, 0);
      expect(narrowed.hasMore, isFalse);
    });
  });
}
