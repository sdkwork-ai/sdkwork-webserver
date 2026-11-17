import type { AppKind } from './app-kind';
import type { ApplicationStoreListing } from './application-store-listing';

export interface CreateApplicationRequest {
  name: string;
  slug?: string;
  description?: string;
  appKind: AppKind;
  /** The `deploy_app.id` of the sdkwork-deployments application this
carrier record carries.

Required. Deploy is the single application identity and publishing
authority; a webserver application exists only to carry a site,
domain, certificate, and reverse-proxy configuration for a Deploy
application, and the link is 1:1 (unique per live carrier row). The
service rejects a missing value with a validation error that names
this field - the webserver never mints an application identity of
its own.
 */
  deployApplicationId: string;
  runtimeConfig?: Record<string, unknown>;
  storeListing?: ApplicationStoreListing;
}
