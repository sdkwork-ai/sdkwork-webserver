const runtimeEnv = require("./runtime/runtime-env");

App({
  globalData: {
    sdkworkRuntimeTarget: runtimeEnv.SDKWORK_RUNTIME_TARGET,
    sdkworkProfileId: runtimeEnv.SDKWORK_PROFILE_ID,
    sdkworkEnvironment: runtimeEnv.SDKWORK_ENVIRONMENT,
    webserverAppApiBaseUrl: runtimeEnv.SDKWORK_WEBSERVER_APP_API_BASE_URL,
    webserverDeployAppApiBaseUrl: runtimeEnv.SDKWORK_WEBSERVER_DEPLOY_APP_API_BASE_URL,
    sdkworkLocale: "",
    sdkworkBootstrapError: "",
  },
  onLaunch() {
    // The bundle is produced by `pnpm build:mini-program`; a missing bundle is a
    // build error, so the failure is recorded rather than thrown out of onLaunch
    // (which would leave the console with no page at all).
    try {
      const runtime = require("./runtime/webserver-app");
      runtime.bootstrapWebserverMiniProgram({ runtimeConfig: runtimeEnv });
      this.globalData.sdkworkLocale = runtime.currentLocale();
    } catch (error) {
      this.globalData.sdkworkBootstrapError = error && error.message
        ? String(error.message)
        : String(error);
    }
  },
});
