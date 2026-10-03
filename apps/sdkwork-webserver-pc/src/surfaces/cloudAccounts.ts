import { cloudAccountOptions, type CloudAccountOption } from "@sdkwork/webserver-pc-admin-delivery";
import { useWebserverConsoleSdk } from "@sdkwork/webserver-pc-console-core";
import { useEffect, useState } from "react";

/**
 * The cloud accounts the Domains page may offer, read from the IAM account center.
 *
 * ## Why this lives in the host rather than in the Domains package
 *
 * A root-domain Zone's `cloudAccountId` is a reference into `iam_provider_account`,
 * an IAM-owned resource served by the IAM backend API. IAM's generated clients may
 * only be composed and driven by a host *core* package (frontend composition: a
 * capability package never imports a generated SDK), and the port to reach them is
 * the `SdkworkIamService` facade this app already composes in console-core and
 * publishes through `WebserverConsoleSdkProvider` — the same port the admin cloud
 * account page and the Identity & Access pages read. So the host does the reading
 * and hands the Domains page labels; the page keeps the filter and its narrowing
 * rule, and owns no transport.
 *
 * The parameter type is declared structurally rather than imported: this module
 * needs the two members it sends, and reaching for the generated client type would
 * be the boundary import the composition rule forbids to a non-core package.
 */

/** The port this module needs, as the IAM client already provides it. */
interface ProviderAccountListPort {
  list(params: { pageSize: number; page: number }): Promise<unknown>;
}

/**
 * The accounts the caller may name, read once per session.
 *
 * A failed read is an empty list rather than an error: the Domains page's banner is
 * for failures that stop it doing its job, and this list is an *offer*. The filter
 * still works without it — "all" and "unassigned" are the page's own states and
 * need no inventory — and a binding already stored on a row is still shown from the
 * row itself, so a caller without `iam.provider_accounts.read` loses suggestions,
 * not function.
 *
 * `pageSize` is the contract's maximum: this is a picker, and a second page of
 * accounts behind a control with no paging is a truncation nobody would notice.
 * The ceiling is bounded by the account center's own page limit, so the read
 * cannot grow without bound.
 */
export function useCloudAccountOptions(): CloudAccountOption[] {
  const { iam } = useWebserverConsoleSdk();
  const [accounts, setAccounts] = useState<CloudAccountOption[]>([]);

  useEffect(() => {
    let active = true;
    const port = (iam.backend.iam as unknown as { providerAccounts: ProviderAccountListPort })
      .providerAccounts;
    void port
      .list({ page: 1, pageSize: 200 })
      .then((response) => {
        if (active) setAccounts(cloudAccountOptions(response));
      })
      .catch(() => {
        if (active) setAccounts([]);
      });
    return () => {
      active = false;
    };
  }, [iam]);

  return accounts;
}
