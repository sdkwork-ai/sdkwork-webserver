/// Session and token store for the Flutter mobile root.
///
/// Authority: `APP_SDK_INTEGRATION_SPEC.md`. Logout, refresh failure, tenant
/// switch, and account switch must clear this store together with the token
/// manager, secure platform storage, and the realtime/session bridges
/// (`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md`).
///
/// Capability packages never read or write tokens; they receive an
/// already-constructed SDK port from core.
library;

class WebserverFlutterSession {
  const WebserverFlutterSession({
    this.accessToken,
    this.authToken,
    this.tenantId,
    this.organizationId,
    this.userId,
  });

  final String? accessToken;
  final String? authToken;
  final String? tenantId;
  final String? organizationId;
  final String? userId;
}

typedef WebserverFlutterSensitiveStateClearer = void Function();

final List<WebserverFlutterSensitiveStateClearer> _sensitiveStateClearers =
    <WebserverFlutterSensitiveStateClearer>[];

/// Register a cache that must be dropped on logout.
///
/// [clearWebserverFlutterSession] runs every registered clearer, so a logout
/// drops the session record, the token projections, and every capability cache
/// in one call instead of relying on each screen to notice.
void registerWebserverFlutterSensitiveStateClearer(
  WebserverFlutterSensitiveStateClearer clearer,
) {
  if (!_sensitiveStateClearers.contains(clearer)) {
    _sensitiveStateClearers.add(clearer);
  }
}

/// Drop every registered cache and forget the registrations.
void clearRegisteredWebserverFlutterSensitiveState() {
  for (final clearer in _sensitiveStateClearers) {
    clearer();
  }
  _sensitiveStateClearers.clear();
}

WebserverFlutterSession? _currentSession;

WebserverFlutterSession? readWebserverFlutterSession() => _currentSession;

String? _normalizeToken(String? value) {
  final trimmed = (value ?? '').trim();
  return trimmed.isEmpty ? null : trimmed;
}

/// Store a session, dropping it entirely when it carries no usable token.
///
/// A session with neither token is not "anonymous but present" — it is absent,
/// and reporting it as authenticated would let a route guard pass.
WebserverFlutterSession? writeWebserverFlutterSession(
  WebserverFlutterSession? session,
) {
  if (session == null) {
    _currentSession = null;
    return null;
  }
  final accessToken = _normalizeToken(session.accessToken);
  final authToken = _normalizeToken(session.authToken);
  if (accessToken == null && authToken == null) {
    _currentSession = null;
    return null;
  }
  final next = WebserverFlutterSession(
    accessToken: accessToken,
    authToken: authToken,
    tenantId: _normalizeToken(session.tenantId),
    organizationId: _normalizeToken(session.organizationId),
    userId: _normalizeToken(session.userId),
  );
  _currentSession = next;
  return next;
}

bool isWebserverFlutterSessionAuthenticated() {
  final session = _currentSession;
  return session?.accessToken != null && session?.authToken != null;
}

/// Full sensitive-state teardown: session record, then every registered cache.
void clearWebserverFlutterSession() {
  _currentSession = null;
  clearRegisteredWebserverFlutterSensitiveState();
}
