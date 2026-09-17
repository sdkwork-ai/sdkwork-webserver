/**
 * Design tokens for the mini program surface.
 *
 * Authority: `APP_MINI_PROGRAM_UI_SPEC.md`. Domain-neutral only; business
 * screens belong to capability packages. Sizes use `rpx` so the tokens scale
 * with the WeChat viewport instead of assuming a pixel width.
 */
export const webserverMpTokens = {
  colorPrimary: "#0f766e",
  colorBackground: "#f8fafc",
  colorSurface: "#ffffff",
  colorText: "#0f172a",
  colorTextMuted: "#64748b",
  colorDanger: "#dc2626",
  spacingSm: "16rpx",
  spacingMd: "32rpx",
  spacingLg: "48rpx",
  radiusMd: "16rpx",
} as const;

export type WebserverMpTokens = typeof webserverMpTokens;
