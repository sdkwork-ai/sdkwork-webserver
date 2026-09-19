/// SDK client workspaces this root declares.
///
/// Authority: `specs/component.spec.json` `contracts.sdkDependencies`.
///
/// `dartArtifactAvailable` records whether a generated Dart/Flutter variant of
/// that workspace exists today. It is deliberately part of the inventory rather
/// than a comment: neither `sdkwork-drive` nor `sdkwork-deployments` ships a
/// Dart variant today — both are TypeScript-only — and the `deploy_app` catalog
/// port in `sdk/` is unbound for exactly that reason.
///
/// The legacy `sdkwork-webserver-app-sdk` family is deliberately absent: the
/// `deploy_app` entity is owned by `sdkwork-deployments`, so this root no longer
/// composes the webserver-owned application surface
/// (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7: one owner per normalized route).
class WebserverFlutterCoreSdkInventoryEntry {
  const WebserverFlutterCoreSdkInventoryEntry({
    required this.workspace,
    required this.packageName,
    required this.permissionModuleId,
    required this.dartArtifactAvailable,
  });

  final String workspace;
  final String packageName;
  final String permissionModuleId;
  final bool dartArtifactAvailable;
}

const List<WebserverFlutterCoreSdkInventoryEntry>
webserverFlutterCoreSdkInventory = <WebserverFlutterCoreSdkInventoryEntry>[
  WebserverFlutterCoreSdkInventoryEntry(
    workspace: 'sdkwork-drive-app-sdk',
    packageName: 'sdkwork_drive_app_sdk',
    permissionModuleId: 'drive',
    dartArtifactAvailable: false,
  ),
  WebserverFlutterCoreSdkInventoryEntry(
    workspace: 'sdkwork-deployments-app-sdk',
    packageName: 'sdkwork_deployments_app_sdk',
    permissionModuleId: 'deployments',
    dartArtifactAvailable: false,
  ),
];

List<WebserverFlutterCoreSdkInventoryEntry> listWebserverFlutterCoreSdkInventory() =>
    webserverFlutterCoreSdkInventory;
