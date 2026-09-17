import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_webserver_flutter_mobile_applications/sdkwork_webserver_flutter_mobile_applications.dart';
import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart'
    show WebserverDeployAppRecord;

/// Requires the Flutter toolchain (`flutter test`). It is committed so the
/// contract travels with the source; `tests/flutter-surface-contract.test.mjs`
/// asserts that these behaviours stay scheduled.
void main() {
  group('record mapping', () {
    test('drops unrenderable records and never leaves a cell blank', () {
      expect(mapWebserverFlutterApplicationRecord(null), isNull);
      expect(
        mapWebserverFlutterApplicationRecord(_record(id: '')),
        isNull,
        reason: 'a record with no identity cannot be rendered as a row',
      );

      final mapped = mapWebserverFlutterApplicationRecord(
        _record(id: 'app-1', name: '', platformTargetCount: -3),
      )!;
      expect(
        mapped.name,
        'app-1',
        reason: 'a record with no display name falls back to its id, not to blank',
      );
      expect(mapped.platformTargetCount, 0);
      expect(mapped.description, '');
      expect(mapped.latestReleaseTag, '');
    });

    test('projects a row whose labels come from the injected label tables', () {
      final row = toWebserverFlutterApplicationRow(
        mapWebserverFlutterApplicationRecord(
          _record(id: 'app-1', appKind: 'SPA_WEB', appStatus: 'ACTIVE'),
        )!,
        webserverFlutterApplicationKindLabels,
        webserverFlutterApplicationStatusLabels,
        (key) => key,
        'applications.list.row.unknown',
      );

      expect(row.kindLabel, 'applications.list.kind.spa-web');
      expect(row.statusLabel, 'applications.list.status.active');
    });

    test('a token outside the closed set renders the unknown label, not the raw token', () {
      final row = toWebserverFlutterApplicationRow(
        mapWebserverFlutterApplicationRecord(
          _record(
            id: 'app-1',
            appKind: 'SOMETHING_NEW',
            appStatus: 'SOMETHING_NEW',
          ),
        )!,
        webserverFlutterApplicationKindLabels,
        webserverFlutterApplicationStatusLabels,
        (key) => key,
        'applications.list.row.unknown',
      );

      expect(row.kindLabel, 'applications.list.row.unknown');
      expect(row.statusLabel, 'applications.list.row.unknown');
      expect(
        lookupWebserverFlutterEnumMessageKey(const [], 'X', 'fallback'),
        'fallback',
      );
    });
  });

  group('authored copy', () {
    test('both locales cover exactly the same keys', () {
      expect(
        webserverFlutterApplicationsMessagesEnUs.keys.toSet(),
        webserverFlutterApplicationsMessagesZhCn.keys.toSet(),
      );
    });

    test('every closed-set label resolves to copy that exists in both locales', () {
      for (final label in <WebserverFlutterEnumLabel>[
        ...webserverFlutterApplicationKindLabels,
        ...webserverFlutterApplicationStatusLabels,
      ]) {
        expect(
          webserverFlutterApplicationsMessagesEnUs,
          contains(label.messageKey),
          reason: 'en-US must define ${label.messageKey}',
        );
        expect(
          webserverFlutterApplicationsMessagesZhCn,
          contains(label.messageKey),
          reason: 'zh-CN must define ${label.messageKey}',
        );
      }
      expect(
        webserverFlutterApplicationsMessagesEnUs,
        contains('applications.list.row.unknown'),
      );
      expect(
        webserverFlutterApplicationsMessagesZhCn,
        contains('applications.list.row.unknown'),
      );
      // The unbound-transport state has its own copy; without it the screen
      // would have to reuse the generic failure message and misreport a
      // declared gap as a request failure.
      expect(
        webserverFlutterApplicationsMessagesEnUs,
        contains('applications.list.unavailable'),
      );
      expect(
        webserverFlutterApplicationsMessagesZhCn,
        contains('applications.list.unavailable'),
      );
    });
  });
}

WebserverDeployAppRecord _record({
  required String id,
  String name = 'Console',
  String slug = 'console',
  String appKind = 'STATIC_WEB',
  String appStatus = 'DRAFT',
  String description = '',
  String defaultEnvironment = '',
  String latestReleaseTag = '',
  int platformTargetCount = 0,
  String updatedAt = '2026-09-17T00:00:00Z',
}) {
  return WebserverDeployAppRecord(
    id: id,
    name: name,
    slug: slug,
    appKind: appKind,
    appStatus: appStatus,
    description: description,
    defaultEnvironment: defaultEnvironment,
    latestReleaseTag: latestReleaseTag,
    platformTargetCount: platformTargetCount,
    updatedAt: updatedAt,
  );
}
