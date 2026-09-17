/// Capability modules this core composes.
///
/// Filled in when a capability is mounted; an empty map means "nothing mounted
/// yet", which is the state of every generated root before the first feature
/// lands.
Map<String, Object?> createWebserverFlutterCoreModuleRegistry() =>
    const <String, Object?>{};
