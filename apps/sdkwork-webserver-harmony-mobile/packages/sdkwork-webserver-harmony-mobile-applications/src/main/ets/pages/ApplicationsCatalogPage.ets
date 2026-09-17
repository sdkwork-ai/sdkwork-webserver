import { WebserverHarmonyEmptyState } from '@sdkwork/sdkwork-webserver-harmony-mobile-commons';

import type { WebserverHarmonyApplicationsScreenModel } from '../../models/ApplicationModels';

/**
 * Route-level capability page for the applications catalog.
 *
 * Root pages mount this; business UI lives in the capability package
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §2 root thinness). The page is a pure
 * projection of the view model's screen model, so the load/append/error state
 * machine stays testable without the ArkUI runtime.
 */
@Component
export struct ApplicationsCatalogPage {
  @Prop title: string = 'Applications';
  @Prop model: WebserverHarmonyApplicationsScreenModel = {
    rows: [],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: '',
    errorDetail: '',
    page: 1,
    totalItems: 0,
    emptyMessage: '',
    loadingMessage: '',
    loadingMoreMessage: '',
    retryLabel: '',
    loadMoreLabel: '',
  };

  build() {
    Column() {
      Text(this.title)
        .fontSize(20)
        .fontWeight(FontWeight.Bold)
        .margin({ bottom: 16 })

      if (this.model.loading) {
        Text(this.model.loadingMessage)
          .fontSize(14)
          .fontColor('#64748B')
      } else if (this.model.errorMessage.length > 0) {
        WebserverHarmonyEmptyState({ message: this.model.errorMessage })
      } else if (this.model.rows.length === 0) {
        WebserverHarmonyEmptyState({ message: this.model.emptyMessage })
      } else {
        List() {
          ForEach(this.model.rows, (row: WebserverHarmonyApplicationsScreenModel['rows'][number]) => {
            ListItem() {
              Column() {
                Text(row.name)
                  .fontSize(16)
                  .fontWeight(FontWeight.Medium)
                Text(`${row.kindLabel} · ${row.statusLabel}`)
                  .fontSize(12)
                  .fontColor('#64748B')
                  .margin({ top: 4 })
              }
              .alignItems(HorizontalAlign.Start)
              .width('100%')
              .padding(16)
            }
          }, (row: WebserverHarmonyApplicationsScreenModel['rows'][number]) => row.id)
        }
        .width('100%')
        .layoutWeight(1)
      }
    }
    .width('100%')
    .height('100%')
    .padding(16)
  }
}
