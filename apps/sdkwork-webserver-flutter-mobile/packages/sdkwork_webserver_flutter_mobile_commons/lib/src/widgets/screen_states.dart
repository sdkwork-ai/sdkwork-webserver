import 'package:flutter/material.dart';

import '../theme/design_tokens.dart';

/// Domain-neutral screen/list state primitives.
///
/// `FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` section 4 — commons owns the shared
/// primitives, capability packages own the pages and map their own payloads onto
/// these payload-free widgets.
enum WebserverFlutterScreenStatus { loading, ready, empty, error }

/// Collapse an item count, an in-flight flag, and an error message into the one
/// status every list screen renders.
///
/// Pure so the contract test can exercise the state machine without a widget
/// binding; it mirrors `resolveWebserverHarmonyScreenStatus` and the H5/mini
/// program resolvers.
WebserverFlutterScreenStatus resolveWebserverFlutterScreenStatus(
  int itemCount,
  bool loading,
  String errorMessage,
) {
  if (loading) {
    return WebserverFlutterScreenStatus.loading;
  }
  if (errorMessage.isNotEmpty) {
    return WebserverFlutterScreenStatus.error;
  }
  return itemCount == 0
      ? WebserverFlutterScreenStatus.empty
      : WebserverFlutterScreenStatus.ready;
}

/// Message key a given status renders. Keeping the mapping here means a new
/// status cannot be added without deciding what it says.
String resolveWebserverFlutterScreenMessageKey(
  WebserverFlutterScreenStatus status,
) {
  switch (status) {
    case WebserverFlutterScreenStatus.loading:
      return 'applications.list.loading';
    case WebserverFlutterScreenStatus.error:
      return 'applications.list.error';
    case WebserverFlutterScreenStatus.empty:
      return 'applications.list.empty';
    case WebserverFlutterScreenStatus.ready:
      return '';
  }
}

/// Payload-free status view used by capability screens.
class WebserverFlutterStatusView extends StatelessWidget {
  const WebserverFlutterStatusView({super.key, required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(WebserverFlutterTokens.spacingLg),
        child: Text(
          message,
          textAlign: TextAlign.center,
          style: const TextStyle(
            color: Color(WebserverFlutterTokens.colorTextMuted),
          ),
        ),
      ),
    );
  }
}
