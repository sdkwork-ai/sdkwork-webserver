/// SDK client workspaces this root declares.
///
/// Authority: `specs/component.spec.json` `contracts.sdkDependencies`.
///
/// `dartArtifactAvailable` records whether a generated Dart/Flutter variant of
/// that workspace exists today. It is deliberately part of the inventory rather
/// than a comment: `sdkwork-webserver` ships a Flutter variant, `sdkwork-drive`
/// and `sdkwork-deployments` ship TypeScript only, and the deploy_app catalog
/// port in `sdk/` is unbound for exactly that reason.
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
    workspace: 'sdkwork-webserver-app-sdk',
    packageName: 'sdkwork_webserver_app_sdk',
    permissionModuleId: 'web',
    dartArtifactAvailable: true,
  ),
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
