/// Use-case orchestration for the applications catalog.
///
/// The reader is injected by root bootstrap; this service maps records and
/// interprets pagination only, and never constructs a transport
/// (`APP_SDK_INTEGRATION_SPEC.md` section 2).
library;

import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart';

import '../models/application_models.dart';
import '../models/application_record_mapping.dart';

class WebserverFlutterApplicationsPage {
  const WebserverFlutterApplicationsPage({
    required this.items,
    required this.page,
  });

  final List<WebserverFlutterApplicationItem> items;
  final WebserverFlutterListPage page;
}

/// One-page-at-a-time `deploy_app` reader.
///
/// `PAGINATION_SPEC.md` forbids full-set loads for interactive lists, so this
/// service asks the server for a single page and returns the canonical
/// `pageInfo` it got back rather than accumulating a total of its own.
class WebserverFlutterApplicationsService {
  WebserverFlutterApplicationsService({
    required this.reader,
    this.pageSize = defaultWebserverFlutterListPageSize,
  });

  final WebserverDeployAppCatalogReader reader;
  final int pageSize;

  int get resolvedPageSize => pageSize > 0 ? pageSize : defaultWebserverFlutterListPageSize;

  Future<WebserverFlutterApplicationsPage> loadPage(
    int page, {
    int? pageSizeOverride,
  }) async {
    if (page < 1) {
      throw ArgumentError.value(page, 'page', 'must be a positive integer');
    }
    final size = pageSizeOverride != null && pageSizeOverride > 0
        ? pageSizeOverride
        : resolvedPageSize;
    final response = await reader.list(page: page, pageSize: size);
    final items = <WebserverFlutterApplicationItem>[];
    for (final record in response.items) {
      final mapped = mapWebserverFlutterApplicationRecord(record);
      if (mapped != null) {
        items.add(mapped);
      }
    }
    return WebserverFlutterApplicationsPage(
      items: items,
      page: toWebserverFlutterListPage(response.pageInfo),
    );
  }
}
