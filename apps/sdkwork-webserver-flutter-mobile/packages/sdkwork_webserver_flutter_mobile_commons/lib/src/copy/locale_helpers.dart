/// Thin locale helpers for the Web Server Flutter mobile capability family.
///
/// Placement note (`I18N_SPEC.md` section 6.1): the authored Flutter fragment
/// layout is `lib/src/i18n/<locale>/<domain>/<capability>/<screen-or-widget>.arb`
/// or `.json` — `.dart` is not an authored fragment extension. Dart code that
/// normalizes a locale or looks a key up in an already-loaded fragment is a
/// boundary helper, not a locale resource, so it lives outside `lib/src/i18n/`.
/// The `gen_l10n` projection that generates Dart accessors from `.arb` fragments
/// requires the Flutter toolchain and is tracked as pending integration.
enum WebserverFlutterLocale { enUs, zhCn }

/// The locale SDKWork ships for `en` is `en-US`; everything else falls back to
/// `en-US` rather than to an empty catalog.
WebserverFlutterLocale normalizeWebserverFlutterLocale(String value) {
  final normalized = value.trim().toLowerCase();
  return normalized.startsWith('zh')
      ? WebserverFlutterLocale.zhCn
      : WebserverFlutterLocale.enUs;
}

/// Normalize a platform-reported locale tag (e.g. `zh_Hans_CN`) the same way.
WebserverFlutterLocale normalizeWebserverFlutterPlatformLocale(String value) {
  return normalizeWebserverFlutterLocale(value.replaceAll('_', '-'));
}

String pickWebserverFlutterMessage(
  Map<String, String> messages,
  String key,
  String fallback,
) {
  final value = messages[key];
  return value != null && value.isNotEmpty ? value : fallback;
}
