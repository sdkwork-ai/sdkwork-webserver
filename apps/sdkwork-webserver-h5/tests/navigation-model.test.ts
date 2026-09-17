import { describe, expect, it } from "vitest";

import {
  APPLICATIONS_PATH,
  APPLICATIONS_ROUTE,
  WEBSERVER_H5_APPLICATIONS_NAVIGATION,
  WEBSERVER_H5_HOME_PATH,
  WEBSERVER_H5_SHELL_NAVIGATION,
  createWebserverH5Navigation,
  filterNavigationForPermission,
} from "@sdkwork/webserver-h5-shell";

describe("h5 navigation model", () => {
  it("keeps the cross-client capability segment and the mobile path in sync", () => {
    expect(APPLICATIONS_ROUTE).toBe("applications");
    expect(APPLICATIONS_PATH).toBe("/applications");
    expect(WEBSERVER_H5_HOME_PATH).toBe(APPLICATIONS_PATH);
    expect(WEBSERVER_H5_APPLICATIONS_NAVIGATION[0]?.id).toBe(APPLICATIONS_ROUTE);
  });

  it("merges contributions in ascending order", () => {
    const merged = createWebserverH5Navigation([
      WEBSERVER_H5_APPLICATIONS_NAVIGATION,
      [
        {
          id: "dashboard",
          labelKey: "navigation.dashboard",
          order: 5,
          path: "/dashboard",
          permission: "deploy.apps.read",
        },
      ],
      WEBSERVER_H5_SHELL_NAVIGATION,
    ]);
    expect(merged.map((entry) => entry.id)).toEqual(["dashboard", "applications"]);
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
