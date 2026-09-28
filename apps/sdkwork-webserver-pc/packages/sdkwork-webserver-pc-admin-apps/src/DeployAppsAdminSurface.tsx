import {
  DeployAppsManagementSurface,
  type DeployAppsManagementSurfaceProps,
} from "@sdkwork/webserver-pc-console-delivery";

/**
 * Props of the backend-admin applications page.
 *
 * The console and the admin answer the same question from the same session facts,
 * so this is the console's prop type rather than a parallel declaration: a second
 * shape would be a second thing to keep in step, and the two hosts already pass
 * exactly the same four values.
 */
export type DeployAppsAdminSurfaceProps = DeployAppsManagementSurfaceProps;

/**
 * The platform admin's applications page.
 *
 * `deploy_app` has one owner (`sdkwork-deployments`) and one route set, so the
 * admin surface renders the console's page instead of a second implementation —
 * the same act the tenant console performs when it renders deployments' page. This
 * component exists for one reason: to mark the render as the admin one.
 *
 * That marker is not cosmetic. The admin face reaches **every** ownership level
 * while the tenant console reaches only the caller's own apps, and the page turns
 * that difference into one control: the admin gets an ownership tab row that is
 * pushed down as `apps.list`'s `scope`, the console gets no control for the axis
 * at all. Nothing here decides that — `surface` is forwarded to the bridge, which
 * forwards it to the page, so a host cannot half-configure it. `surface="admin"`
 * is written after the spread so it always wins, whatever a caller passed.
 *
 * It deliberately does **not** widen anything else. What levels a caller may read
 * is the server's ownership gate (`app_owner_gate` defaults to platform + tenant +
 * the caller's own user/organization), and `scope` can only narrow that answer —
 * the same division the console relies on. Offering a level here that the server
 * refuses would be a second, local copy of that rule, and the copy that drifts is
 * always the one that keeps offering a level the server refuses.
 */
export function DeployAppsAdminSurface(props: DeployAppsAdminSurfaceProps) {
  return <DeployAppsManagementSurface {...props} surface="admin" />;
}
