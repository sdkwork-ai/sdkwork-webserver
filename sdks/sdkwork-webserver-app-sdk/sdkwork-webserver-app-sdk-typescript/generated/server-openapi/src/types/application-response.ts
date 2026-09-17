import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface ApplicationResponse {
  id?: string;
  name?: string;
  slug?: string;
  description?: string;
  /** The application's backing site id (internal carrier) */
  siteId?: string;
  appKind?: AppKind;
  /** Internal carrier site type derived from the app kind */
  siteType?: number;
  status?: number;
  /** 该应用是否已有源码版本。由 application 列表/详情投影用一次查询算出
（`EXISTS` over `web_source_version`），调用方无需再逐行探测
`applications/{applicationId}/source_versions`。
 */
  hasSourceVersion?: boolean;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
  createdAt?: string;
  updatedAt?: string;
}
