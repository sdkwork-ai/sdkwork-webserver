export function listWebserverCoreSdkInventory() {
  return [
    { packageName: "@sdkwork/deployments-app-sdk", authority: "sdkwork-deploy-app-api", surface: "app-api" },
    { packageName: "@sdkwork/drive-app-sdk", authority: "sdkwork-drive-app-api", surface: "app-api" },
  ] as const;
}
