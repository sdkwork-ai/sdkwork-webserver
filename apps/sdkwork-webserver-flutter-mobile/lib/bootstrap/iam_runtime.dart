/// IAM projection for the Flutter mobile root.
///
/// Permission codes are the codes the owning modules derive from their own
/// operationIds (`[resource, action] = operationId.split(".")`); this root never
/// invents one, and it never ships a local catalog for a dependency domain
/// (`APP_COMPOSITION_SPEC.md`, `permissionComposition.consumerPolicy`).
library;

class WebserverFlutterIamRuntime {
  WebserverFlutterIamRuntime({List<String> grantedPermissions = const <String>[]})
    : _granted = <String>{...grantedPermissions};

  final Set<String> _granted;

  Set<String> get grantedPermissions => Set<String>.unmodifiable(_granted);

  void setGrantedPermissions(Iterable<String> permissions) {
    _granted
      ..clear()
      ..addAll(permissions);
  }

  /// Rebuild the projection from a freshly issued access token.
  void applyAccessTokenScope(Iterable<String> scopes) {
    setGrantedPermissions(scopes);
  }

  bool hasPermission(String permission) => _granted.contains(permission);

  /// Drop every grant on logout, tenant switch, or account switch.
  void clearOnLogout() {
    _granted.clear();
  }
}

WebserverFlutterIamRuntime createWebserverFlutterIamRuntime(
  List<String> grantedPermissions,
) {
  return WebserverFlutterIamRuntime(grantedPermissions: grantedPermissions);
}
