export function listWebserverCoreSdkInventory() {
  return [
    { packageName: "@sdkwork/webserver-app-sdk", authority: "sdkwork-webserver-app-api", surface: "app-api" },
    { packageName: "@sdkwork/drive-app-sdk", authority: "sdkwork-drive-app-api", surface: "app-api" },
  ] as const;
}
