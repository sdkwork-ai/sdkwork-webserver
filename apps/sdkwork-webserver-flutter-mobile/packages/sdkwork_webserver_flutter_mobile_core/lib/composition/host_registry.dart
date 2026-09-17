/// Host adapters bound by the root, keyed by capability.
///
/// Flutter resolves its host surface through plugin channels registered at the
/// application root, not from a package, so core only carries the registry the
/// root fills in.
Map<String, Object?> createWebserverFlutterCoreHostRegistry() =>
    const <String, Object?>{};
