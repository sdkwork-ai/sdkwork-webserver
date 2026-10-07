export interface WebserverAuthHostMessages {
  backToPortal: string;
  metadataConnecting: string;
  metadataUnavailable: string;
  retry: string;
  sessionChecking: string;
  switchToDarkMode: string;
  switchToLightMode: string;
}

const AUTH_HOST_MESSAGES: Record<"en-US" | "zh-CN", WebserverAuthHostMessages> = {
  "en-US": {
    backToPortal: "Back to Portal home",
    metadataConnecting: "Connecting to the identity service...",
    metadataUnavailable: "The identity service is currently unavailable.",
    retry: "Retry",
    sessionChecking: "Checking your session...",
    switchToDarkMode: "Switch to dark mode",
    switchToLightMode: "Switch to light mode",
  },
  "zh-CN": {
    backToPortal: "返回 Portal 首页",
    metadataConnecting: "正在连接身份服务...",
    metadataUnavailable: "身份服务暂时不可用。",
    retry: "重试",
    sessionChecking: "正在验证登录状态...",
    switchToDarkMode: "切换到深色模式",
    switchToLightMode: "切换到浅色模式",
  },
};

export function resolveWebserverAuthHostMessages(locale: string): WebserverAuthHostMessages {
  return locale.toLowerCase().startsWith("zh")
    ? AUTH_HOST_MESSAGES["zh-CN"]
    : AUTH_HOST_MESSAGES["en-US"];
}
