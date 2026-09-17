/// Design tokens for the Flutter mobile surface.
///
/// Authority: `APP_FLUTTER_UI_SPEC.md`. Domain-neutral only; business screens
/// belong to capability packages and shared packages must not import an
/// application app shell. Values match the H5, mini program, and HarmonyOS roots
/// so the same console reads as one product on every client.
class WebserverFlutterTokens {
  const WebserverFlutterTokens._();

  static const int colorPrimary = 0xFF0F766E;
  static const int colorBackground = 0xFFF8FAFC;
  static const int colorSurface = 0xFFFFFFFF;
  static const int colorText = 0xFF0F172A;
  static const int colorTextMuted = 0xFF64748B;
  static const int colorDanger = 0xFFDC2626;

  static const double spacingSm = 8;
  static const double spacingMd = 16;
  static const double spacingLg = 24;
  static const double radiusMd = 8;
}
