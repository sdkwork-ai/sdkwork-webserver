/**
 * Applications list page — native projection.
 *
 * This file is deliberately thin. Every state transition lives in
 * `@sdkwork/webserver-mp-applications` (`pages/applicationListPageModel`), the SDK
 * client is created by the runtime bundle, and platform calls go through the host
 * adapter the bundle exposes. What is left here is the platform binding itself:
 * lifecycle hooks and `setData`.
 */
const runtime = require("../../runtime/webserver-app");

function bootstrapError() {
  const app = getApp();
  return typeof app?.globalData?.sdkworkBootstrapError === "string"
    ? app.globalData.sdkworkBootstrapError
    : "";
}

function createBinding(page) {
  return runtime.createApplicationsListPageBinding({
    onDataChange: (data) => page.setData(data),
    onSettled: () => runtime.stopPullDownRefresh(),
  });
}

function initialData() {
  const app = getApp();
  return {
    items: [],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: "",
    errorDetail: "",
    page: 1,
    totalItems: 0,
    emptyMessage: "",
    loadingMessage: "",
    loadingMoreMessage: "",
    retryLabel: "",
    loadMoreLabel: "",
    brand: "",
    title: "SDKWork Web Server",
    fatalError: bootstrapError(),
  };
}

Page({
  data: initialData(),
  binding: null,

  onLoad() {
    if (this.data.fatalError) {
      this.setData({ loading: false });
      return;
    }
    try {
      this.binding = createBinding(this);
    } catch (error) {
      this.setData({
        loading: false,
        errorMessage: "SDKWork Web Server failed to start.",
        errorDetail: error && error.message ? String(error.message) : String(error),
      });
      return;
    }
    if (!this.binding.enterable) {
      this.setData({
        loading: false,
        errorMessage: this.binding.message("applications.list.error"),
        errorDetail: this.binding.blockedReason,
      });
      return;
    }
    this.binding.load();
  },

  onPullDownRefresh() {
    if (this.binding) {
      this.binding.load();
      return;
    }
    runtime.stopPullDownRefresh();
  },

  onReachBottom() {
    if (this.binding) {
      this.binding.loadMore();
    }
  },

  onRetry() {
    if (this.binding) {
      this.binding.load();
    }
  },
});
