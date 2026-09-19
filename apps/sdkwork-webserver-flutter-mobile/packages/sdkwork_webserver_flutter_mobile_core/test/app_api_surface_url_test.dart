import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

/// Surface-URL contract for the app API base URL this root reads from a
/// deployment profile.
///
/// No Dart/Flutter toolchain exists in this workspace, so
/// `tests/flutter-surface-contract.test.mjs` asserts that this file names each
/// behaviour; these cases run once the toolchain lands.
void main() {
  test('accepts one prefixed surface URL and normalizes trailing slashes', () {
    expect(
      normalizeWebserverAppApiBaseUrl(' https://api.example.com/app/v3/api/ '),
      'https://api.example.com/app/v3/api',
    );
  });

  test('rejects a bare origin that carries no surface prefix', () {
    expect(
      () => normalizeWebserverAppApiBaseUrl('https://api.example.com'),
      throwsA(isA<ArgumentError>()),
    );
  });

  test('rejects a URL carrying a duplicated surface prefix', () {
    expect(
      () => normalizeWebserverAppApiBaseUrl(
        'https://api.example.com/app/v3/api/app/v3/api',
      ),
      throwsA(isA<ArgumentError>()),
    );
  });

  test('rejects a URL carrying a query or a fragment', () {
    expect(
      () => normalizeWebserverAppApiBaseUrl(
        'https://api.example.com/app/v3/api?tenant=1',
      ),
      throwsA(isA<ArgumentError>()),
    );
    expect(
      () => normalizeWebserverAppApiBaseUrl(
        'https://api.example.com/app/v3/api#fragment',
      ),
      throwsA(isA<ArgumentError>()),
    );
  });

  test('rejects a non-HTTP(S) scheme', () {
    expect(
      () => normalizeWebserverAppApiBaseUrl('ftp://api.example.com/app/v3/api'),
      throwsA(isA<ArgumentError>()),
    );
  });
}
