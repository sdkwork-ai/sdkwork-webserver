/**
 * The cloud accounts a root-domain form may name.
 *
 * ## Why the list is an injected prop rather than a read from this package
 *
 * A root-domain Zone's `cloudAccountId` is a reference into the account center
 * (`iam_provider_account`, owned by sdkwork-iam), so the accounts that may be
 * named are exactly the ones that center holds. The Web Server's own backend
 * serves no account inventory — it stores the reference and nothing else — and
 * the edge's `/dns_accounts` list is a different thing entirely: those are the
 * accounts *this deployment's configuration file* holds credentials for, which is
 * a deployment fact, not the account center's inventory. Offering those would let
 * an operator bind a Zone to an id the account center has never heard of, and
 * would hide every account this deployment has not been configured with yet.
 *
 * Reading the center means calling IAM, and IAM's generated clients may only be
 * composed and driven by a host *core* package (frontend composition: a capability
 * package never imports a generated SDK). So the host reads the list through the
 * same console SDK provider the admin route tree already sits inside, and hands
 * the Domains page the labels — this package contributes the page and the
 * narrowing rule below, and owns no transport.
 */

/** One account the forms may name. */
export interface CloudAccountOption {
  id: string;
  /** Operator-facing label: the account's display name, falling back to its code. */
  label: string;
}

/**
 * Reads the account options out of the IAM provider-account page.
 *
 * The IAM surface's page items are untyped on the generated client (the IAM
 * contract publishes no item schema), so the shape is narrowed here rather than
 * asserted: an item without a usable id is dropped instead of being rendered as a
 * blank option an operator could select and bind. A blank *label* falls back to
 * the id, because the id is what the binding stores and is still the honest name
 * for the row.
 *
 * Pure and exported on its own so the rule can be tested without a render: it is
 * the one place this page makes a claim about another module's payload.
 */
export function cloudAccountOptions(response: unknown): CloudAccountOption[] {
  const items = (response as { items?: unknown } | null | undefined)?.items;
  if (!Array.isArray(items)) return [];
  return items.flatMap((item) => {
    const record = item as Record<string, unknown> | null;
    const id = typeof record?.id === "string" ? record.id.trim() : "";
    if (id === "") return [];
    const displayName = typeof record?.displayName === "string" ? record.displayName.trim() : "";
    const accountCode = typeof record?.accountCode === "string" ? record.accountCode.trim() : "";
    return [{ id, label: displayName || accountCode || id }];
  });
}
