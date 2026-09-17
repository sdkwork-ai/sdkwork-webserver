/// Narrows generated `pageInfo` payloads onto the list model the Flutter
/// surfaces render.
///
/// `PAGINATION_SPEC.md` forbids client-side full-set loads for interactive
/// lists, so capability packages page through the SDK response and never
/// re-derive their own totals. The rules mirror the H5
/// (`toWebserverH5ListPage`) and mini program (`toWebserverMpListPage`) roots
/// so all four client roots agree on `hasMore`.
class WebserverFlutterListPage {
  const WebserverFlutterListPage({
    required this.hasMore,
    required this.page,
    required this.pageSize,
    required this.totalItems,
    required this.totalPages,
  });

  final bool hasMore;
  final int page;
  final int pageSize;
  final int totalItems;
  final int totalPages;
}

/// One `pageInfo` payload before it is narrowed.
///
/// `totalItems` is `number | string` upstream because the generated contract
/// serializes 64-bit counts as strings; `totalPages` may be absent entirely on
/// cursor-mode responses.
class WebserverFlutterPageInfoLike {
  const WebserverFlutterPageInfoLike({
    this.hasMore,
    this.page,
    this.pageSize,
    this.totalItems,
    this.totalPages,
  });

  final bool? hasMore;
  final int? page;
  final int? pageSize;
  final Object? totalItems;
  final int? totalPages;
}

const int defaultWebserverFlutterListPageSize = 20;

int _normalizeCount(Object? value, int fallback, int minimum) {
  final int parsed;
  if (value is int) {
    parsed = value;
  } else if (value is num) {
    parsed = value.isFinite ? value.toInt() : fallback;
  } else {
    parsed = int.tryParse(value?.toString() ?? '') ?? fallback;
  }
  return parsed < minimum ? minimum : parsed;
}

/// Narrow a generated `pageInfo` onto the list model.
///
/// `hasMore` is taken from the server when it is explicitly `true`, and is
/// otherwise derived from `page < totalPages` — never invented from the number
/// of items this client happens to render.
WebserverFlutterListPage toWebserverFlutterListPage(
  WebserverFlutterPageInfoLike? pageInfo,
) {
  final page = _normalizeCount(pageInfo?.page, 1, 1);
  final pageSize = _normalizeCount(
    pageInfo?.pageSize,
    defaultWebserverFlutterListPageSize,
    1,
  );
  final totalPages = _normalizeCount(pageInfo?.totalPages, 0, 0);
  final totalItems = _normalizeCount(pageInfo?.totalItems, 0, 0);
  return WebserverFlutterListPage(
    hasMore: pageInfo?.hasMore == true || (totalPages > 0 && page < totalPages),
    page: page,
    pageSize: pageSize,
    totalItems: totalItems,
    totalPages: totalPages,
  );
}
