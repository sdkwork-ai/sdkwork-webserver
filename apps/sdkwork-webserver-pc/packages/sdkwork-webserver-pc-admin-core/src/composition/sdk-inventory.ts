export function listWebserverCoreSdkInventory() {
  return [
    { packageName: "@sdkwork/webserver-backend-sdk", authority: "sdkwork-webserver-backend-api", surface: "backend-api" },
  ] as const;
}
