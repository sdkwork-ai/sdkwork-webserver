/// Package-local state slice for the applications catalog.
///
/// `FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` requires sensitive state to clear
/// on logout and on account/tenant switch. The slice registers itself with the
/// core's clearing registry, so a logout drops tenant-scoped rows without every
/// screen having to remember to.
library;

import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

import '../models/application_models.dart';

class WebserverFlutterApplicationsCatalogStateSlice {
  const WebserverFlutterApplicationsCatalogStateSlice({
    required this.page,
    required this.rows,
    required this.hasMore,
    required this.errorMessage,
  });

  final int page;
  final List<WebserverFlutterApplicationRow> rows;
  final bool hasMore;
  final String errorMessage;
}

WebserverFlutterApplicationsCatalogStateSlice
createInitialWebserverFlutterApplicationsCatalogState() {
  return const WebserverFlutterApplicationsCatalogStateSlice(
    page: 1,
    rows: <WebserverFlutterApplicationRow>[],
    hasMore: false,
    errorMessage: '',
  );
}

void registerWebserverFlutterApplicationsCatalogStateClearing(
  void Function() onClear,
) {
  registerWebserverFlutterSensitiveStateClearer(onClear);
}
