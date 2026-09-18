import { describe, expect, it } from "vitest";

import {
  APPLICATIONS_PATH,
  APPLICATIONS_ROUTE,
  APPLICATIONS_ROUTE_ID,
  WEBSERVER_H5_APPLICATIONS_NAVIGATION,
  WEBSERVER_H5_HOME_PATH,
  WEBSERVER_H5_SHELL_NAVIGATION,
  createWebserverH5Navigation,
  filterNavigationForPermission,
} from "@sdkwork/webserver-h5-shell";

describe("h5 navigation model", () => {
  it("carries the canonical cross-client route id next to the short mobile path", () => {
    expect(APPLICATIONS_ROUTE).toBe("applications");
    expect(APPLICATIONS_PATH).toBe("/applications");
    expect(WEBSERVER_H5_HOME_PATH).toBe(APPLICATIONS_PATH);

    // `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` section 7: routes are aligned by
    // identity, not by physical path — so the id is four segments while the path
    // stays short.
    expect(APPLICATIONS_ROUTE_ID).toBe("app.webserver.applications.list");

    const entry = WEBSERVER_H5_APPLICATIONS_NAVIGATION[0];
    expect(entry?.id).toBe(APPLICATIONS_ROUTE_ID);
    expect(entry?.id).toBe(
      `${entry?.surface}.${entry?.domain}.${entry?.capability}.${entry?.screen}`,
    );
    expect(entry?.labelKey).toBe("navigation.applications");
    expect(entry?.titleKey).toBe("applications.list.title");
    expect(entry?.permissionHint).toBe("deploy.apps.read");
  });

  it("refuses a route id that is not <surface>.<domain>.<capability>.<screen>", () => {
    expect(() =>
      createWebserverH5Navigation([
        [
          {
            id: "applications",
            surface: "app",
            domain: "webserver",
            capability: "applications",
            screen: "list",
            path: "/applications",
            labelKey: "navigation.applications",
            titleKey: "applications.list.title",
            permissionHint: "deploy.apps.read",
            order: 10,
          },
        ],
      ]),
    ).toThrow(/must equal app\.webserver\.applications\.list/u);
  });

  it("merges contributions in ascending order", () => {
    const merged = createWebserverH5Navigation([
      WEBSERVER_H5_APPLICATIONS_NAVIGATION,
      [
        {
          id: "app.webserver.dashboard.index",
          surface: "app",
          domain: "webserver",
          capability: "dashboard",
          screen: "index",
          path: "/dashboard",
          labelKey: "navigation.dashboard",
          titleKey: "dashboard.index.title",
          permissionHint: "deploy.apps.read",
          order: 5,
        },
      ],
      WEBSERVER_H5_SHELL_NAVIGATION,
    ]);
    expect(merged.map((entry) => entry.id)).toEqual([
      "app.webserver.dashboard.index",
      "app.webserver.applications.list",
    ]);
  });

  it("refuses a duplicate id or path across contributions", () => {
    expect(() =>
      createWebserverH5Navigation([
        WEBSERVER_H5_APPLICATIONS_NAVIGATION,
        WEBSERVER_H5_APPLICATIONS_NAVIGATION,
      ]),
    ).toThrow(/duplicate H5 navigation/u);
  });

  it("filters entries the viewer is not entitled to", () => {
    const navigation = createWebserverH5Navigation([WEBSERVER_H5_APPLICATIONS_NAVIGATION]);
    expect(filterNavigationForPermission(navigation, () => true)).toHaveLength(1);
    expect(filterNavigationForPermission(navigation, () => false)).toHaveLength(0);
  });
});
