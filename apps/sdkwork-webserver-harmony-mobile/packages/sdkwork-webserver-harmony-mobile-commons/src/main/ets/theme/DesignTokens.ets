/**
 * Design tokens for the HarmonyOS surface.
 *
 * Authority: `APP_HARMONY_NATIVE_UI_SPEC.md`. Domain-neutral only; business
 * screens belong to capability packages. Values match the H5 and mini program
 * roots so the same console reads as one product on every client.
 */
export class WebserverHarmonyTokens {
  static readonly colorPrimary: string = '#0F766E';
  static readonly colorBackground: string = '#F8FAFC';
  static readonly colorSurface: string = '#FFFFFF';
  static readonly colorText: string = '#0F172A';
  static readonly colorTextMuted: string = '#64748B';
  static readonly colorDanger: string = '#DC2626';
  static readonly spacingSm: number = 8;
  static readonly spacingMd: number = 16;
  static readonly spacingLg: number = 24;
  static readonly radiusMd: number = 8;
}
