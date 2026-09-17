/// Host adapter registration for the Flutter mobile root.
///
/// Flutter resolves platform services through plugin channels registered at the
/// application root, so this file is where a plugin-backed adapter is bound.
///
/// Only capabilities the root can honestly serve are registered. A plugin that
/// is not a dependency yet (`flutter_secure_storage` for token persistence, for
/// example) is **left unregistered** rather than backed by an in-memory stub:
/// an in-memory "secure" store would report success and silently lose the token
/// on process death, which is worse than reporting the capability absent.
library;

/// Capabilities this root currently binds.
const List<String> webserverFlutterHostCapabilities = <String>[
  'appLifecycle',
];

/// Register the host adapters. Idempotent.
void registerWebserverFlutterHostAdapters() {
  // No plugin-backed adapter is bound yet; see the library comment for why an
  // unregistered capability is the honest state.
}
