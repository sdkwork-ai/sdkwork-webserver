import { getWebserverHarmonySdkClients } from '../bootstrap/SdkClients';
import { getWebserverHarmonyRoutes } from '../bootstrap/Routes';

/**
 * Root page.
 *
 * The root page stays a thin shell mount point. Real screens are contributed by
 * capability packages (`packages/sdkwork-webserver-harmony-mobile-*`); the only
 * thing this file does is resolve the composed route stack so the shell can hand
 * the first screen to the ArkUI navigation container.
 */
@Entry
@Component
struct Index {
  @State title: string = 'SDKWork Web Server';
  @State subtitle: string = 'Harmony shell mount point';
  @State routeCount: number = 0;

  aboutToAppear(): void {
    getWebserverHarmonySdkClients();
    this.routeCount = getWebserverHarmonyRoutes().length;
  }

  build() {
    Column() {
      Text(this.title)
        .fontSize(24)
        .fontWeight(FontWeight.Bold)
      Text(this.subtitle)
        .fontSize(14)
        .fontColor('#64748B')
        .margin({ top: 8 })
      Text(`routes: ${this.routeCount}`)
        .fontSize(12)
        .fontColor('#94A3B8')
        .margin({ top: 4 })
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
  }
}
