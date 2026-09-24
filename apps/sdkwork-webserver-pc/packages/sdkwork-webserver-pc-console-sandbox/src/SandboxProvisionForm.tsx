import {
  SANDBOX_ISOLATION_ASSURANCES,
  type CreateSandboxInstanceInput,
  type SandboxInstance,
  type SandboxInstanceProfile,
  type SandboxInstanceState,
  type SandboxIsolationAssurance,
  type SandboxRuntimeCapability,
  type UpdateSandboxInstanceInput,
} from "@sdkwork/webserver-pc-console-core";
import { useMemo, useState, type FormEvent } from "react";
import {
  SANDBOX_ASSURANCE_LABEL_KEYS,
  SANDBOX_CAPABILITY_LABEL_KEYS,
  SANDBOX_PROFILE_LABEL_KEYS,
  SANDBOX_STATE_LABEL_KEYS,
  sandboxValidationMessageKey,
} from "./i18n.ts";
import { useSandboxInstancesT } from "./locale.tsx";
import { ConsoleField, ConsoleFormSection } from "./SurfaceOverlay.tsx";
import {
  createSandboxInstanceDraft,
  sandboxDraftFromInstance,
  sandboxProfileBounds,
  sandboxStateTransitions,
  toCreateSandboxInstanceInput,
  toUpdateSandboxInstanceInput,
  toggleSandboxCapability,
  validateSandboxInstanceDraft,
  SANDBOX_CAPABILITY_ORDER,
  SANDBOX_INSTANCE_LIMITS,
  SANDBOX_PROFILE_ORDER,
  type SandboxDraftErrors,
  type SandboxInstanceDraft,
} from "./sandbox-instances-model.ts";

/**
 * What the form hands back once the draft is clean.
 *
 * A discriminated union rather than one `Partial` body: `POST` and `PATCH` are
 * different commands with different required fields (`POST` must carry a base
 * image, `PATCH` cannot), and the surface has to pick a client method anyway. The
 * union makes that choice explicit instead of leaving it to which keys happen to
 * be filled in.
 */
export type SandboxProvisionFormSubmit =
  | { kind: "create"; input: CreateSandboxInstanceInput }
  | { kind: "update"; input: UpdateSandboxInstanceInput };

export interface SandboxProvisionFormProps {
  busy: boolean;
  /** The row being edited; `null` provisions a new instance. */
  instance: SandboxInstance | null;
  onCancel: () => void;
  onSubmit: (submit: SandboxProvisionFormSubmit) => void;
}

export function SandboxProvisionForm({
  busy,
  instance,
  onCancel,
  onSubmit,
}: SandboxProvisionFormProps) {
  const t = useSandboxInstancesT();
  const editing = instance !== null;
  const [draft, setDraft] = useState<SandboxInstanceDraft>(() =>
    instance ? sandboxDraftFromInstance(instance) : createSandboxInstanceDraft());
  const [errors, setErrors] = useState<SandboxDraftErrors>({});
  /**
   * The state change the operator asked for, kept out of the draft: the draft is
   * the resource shape, and a lifecycle transition is a separate command that
   * happens to travel in the same `PATCH`. Empty means "leave the state alone".
   */
  const [nextState, setNextState] = useState<SandboxInstanceState | "">("");

  const bounds = sandboxProfileBounds(draft.profile);
  const profileLabel = t(SANDBOX_PROFILE_LABEL_KEYS[draft.profile]);
  const transitions = instance ? sandboxStateTransitions(instance.sandboxInstanceState) : [];

  /**
   * Message for one field, resolved from the code the pure validator returned.
   * Bounds are interpolated from the *current* profile so a rejection names the
   * envelope that produced it rather than a fixed number.
   */
  const fieldError = useMemo(() => (field: keyof SandboxDraftErrors): string | undefined => {
    const code = errors[field];
    if (!code) return undefined;
    return t(sandboxValidationMessageKey(code), {
      max: field === "name"
        ? SANDBOX_INSTANCE_LIMITS.nameMaxLength
        : field === "baseImage"
          ? SANDBOX_INSTANCE_LIMITS.baseImageMaxLength
          : field === "vcpuCount"
            ? bounds.maxVcpuCount
            : field === "memoryMb"
              ? bounds.maxMemoryMb
              : bounds.maxDiskMb,
      min: field === "vcpuCount"
        ? SANDBOX_INSTANCE_LIMITS.vcpuMinCount
        : field === "memoryMb"
          ? SANDBOX_INSTANCE_LIMITS.memoryMinMb
          : SANDBOX_INSTANCE_LIMITS.diskMinMb,
      profile: profileLabel,
    });
  }, [bounds.maxDiskMb, bounds.maxMemoryMb, bounds.maxVcpuCount, errors, profileLabel, t]);

  function onProfileChange(profile: SandboxInstanceProfile) {
    setDraft((current) => ({ ...current, profile }));
    // Clear the shape verdicts: they were decided against the previous envelope,
    // so keeping them would show "out of the standard envelope" next to a field
    // the operator just moved to a wider profile.
    setErrors((current) => ({
      ...current,
      diskMb: undefined,
      memoryMb: undefined,
      vcpuCount: undefined,
    }));
  }

  function onSubmitForm(event: FormEvent) {
    event.preventDefault();
    const verdict = validateSandboxInstanceDraft(draft, { requireBaseImage: !editing });
    setErrors(verdict);
    if (Object.keys(verdict).length > 0) return;
    onSubmit(editing
      ? {
          kind: "update",
          input: toUpdateSandboxInstanceInput(draft, nextState ? { state: nextState } : {}),
        }
      : { kind: "create", input: toCreateSandboxInstanceInput(draft) });
  }

  return (
    <form className="skills-console-form" onSubmit={onSubmitForm}>
      <ConsoleFormSection title={t("section.shape")}>
        <ConsoleField
          error={fieldError("name")}
          hint={t("hint.name", { max: SANDBOX_INSTANCE_LIMITS.nameMaxLength })}
          htmlFor="sandbox-instance-name"
          label={t("field.name")}
          required
        >
          <input
            id="sandbox-instance-name"
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            placeholder={t("placeholder.name")}
            type="text"
            value={draft.name}
          />
        </ConsoleField>

        {editing ? (
          // The base image is fixed at provisioning time: `PATCH` has no
          // `sandboxInstanceBaseImage` key, so showing an editable box would offer
          // a change the server silently drops.
          <ConsoleField
            hint={t("hint.baseImageImmutable")}
            htmlFor="sandbox-instance-base-image"
            label={t("field.baseImage")}
          >
            <input
              id="sandbox-instance-base-image"
              readOnly
              type="text"
              value={draft.baseImage}
            />
          </ConsoleField>
        ) : (
          <ConsoleField
            error={fieldError("baseImage")}
            hint={t("hint.baseImage")}
            htmlFor="sandbox-instance-base-image"
            label={t("field.baseImage")}
            required
          >
            <input
              id="sandbox-instance-base-image"
              onChange={(event) => setDraft({ ...draft, baseImage: event.target.value })}
              placeholder={t("placeholder.baseImage")}
              type="text"
              value={draft.baseImage}
            />
          </ConsoleField>
        )}

        <ConsoleField hint={t("hint.profile")} htmlFor="sandbox-instance-profile" label={t("field.profile")} required>
          <select
            id="sandbox-instance-profile"
            onChange={(event) => onProfileChange(event.target.value as SandboxInstanceProfile)}
            value={draft.profile}
          >
            {SANDBOX_PROFILE_ORDER.map((profile) => (
              <option key={profile} value={profile}>{t(SANDBOX_PROFILE_LABEL_KEYS[profile])}</option>
            ))}
          </select>
        </ConsoleField>

        <ConsoleField
          error={fieldError("vcpuCount")}
          hint={t("hint.vcpuCount", { max: bounds.maxVcpuCount, min: SANDBOX_INSTANCE_LIMITS.vcpuMinCount, profile: profileLabel })}
          htmlFor="sandbox-instance-vcpu"
          label={t("field.vcpuCount")}
          required
        >
          <input
            id="sandbox-instance-vcpu"
            inputMode="numeric"
            min={SANDBOX_INSTANCE_LIMITS.vcpuMinCount}
            onChange={(event) => setDraft({ ...draft, vcpuCount: event.target.value })}
            type="number"
            value={draft.vcpuCount}
          />
        </ConsoleField>

        <ConsoleField
          error={fieldError("memoryMb")}
          hint={t("hint.memoryMb", { max: bounds.maxMemoryMb, min: SANDBOX_INSTANCE_LIMITS.memoryMinMb, profile: profileLabel })}
          htmlFor="sandbox-instance-memory"
          label={t("field.memoryMb")}
          required
        >
          <input
            id="sandbox-instance-memory"
            inputMode="numeric"
            min={SANDBOX_INSTANCE_LIMITS.memoryMinMb}
            onChange={(event) => setDraft({ ...draft, memoryMb: event.target.value })}
            type="number"
            value={draft.memoryMb}
          />
        </ConsoleField>

        <ConsoleField
          error={fieldError("diskMb")}
          hint={t("hint.diskMb", { max: bounds.maxDiskMb, min: SANDBOX_INSTANCE_LIMITS.diskMinMb, profile: profileLabel })}
          htmlFor="sandbox-instance-disk"
          label={t("field.diskMb")}
          required
        >
          <input
            id="sandbox-instance-disk"
            inputMode="numeric"
            min={SANDBOX_INSTANCE_LIMITS.diskMinMb}
            onChange={(event) => setDraft({ ...draft, diskMb: event.target.value })}
            type="number"
            value={draft.diskMb}
          />
        </ConsoleField>
      </ConsoleFormSection>

      <ConsoleFormSection title={t("section.isolation")}>
        <ConsoleField
          hint={t("hint.assurance")}
          htmlFor="sandbox-instance-assurance"
          label={t("field.assurance")}
          required
        >
          <select
            id="sandbox-instance-assurance"
            onChange={(event) =>
              setDraft({ ...draft, assurance: event.target.value as SandboxIsolationAssurance })}
            value={draft.assurance}
          >
            {SANDBOX_ISOLATION_ASSURANCES.map((assurance) => (
              <option key={assurance} value={assurance}>{t(SANDBOX_ASSURANCE_LABEL_KEYS[assurance])}</option>
            ))}
          </select>
        </ConsoleField>

        <ConsoleField
          hint={t("hint.capabilities", { max: SANDBOX_INSTANCE_LIMITS.capabilitiesMax })}
          label={t("field.capabilities")}
        >
          <div className="sandbox-option-group sandbox-option-group--grid" role="group" aria-label={t("field.capabilities")}>
            {SANDBOX_CAPABILITY_ORDER.map((capability) => (
              <label key={capability}>
                <input
                  checked={draft.requiredCapabilities.includes(capability)}
                  disabled={busy}
                  onChange={() => setDraft({
                    ...draft,
                    requiredCapabilities: toggleSandboxCapability(draft.requiredCapabilities, capability),
                  })}
                  type="checkbox"
                />
                <span>{t(SANDBOX_CAPABILITY_LABEL_KEYS[capability])}</span>
              </label>
            ))}
          </div>
        </ConsoleField>
      </ConsoleFormSection>

      <ConsoleFormSection title={t("section.lifecycle")}>
        {editing ? (
          // A select rather than one button per transition: the offered set is an
          // echo of the server's state machine (`sandboxStateTransitions`), which
          // is empty for a terminal instance — and an empty control is honestly
          // omitted instead of drawn disabled.
          <ConsoleField
            hint={t("hint.nextState")}
            htmlFor="sandbox-instance-next-state"
            label={t("field.nextState")}
          >
            <select
              disabled={busy}
              id="sandbox-instance-next-state"
              onChange={(event) => setNextState(event.target.value as SandboxInstanceState | "")}
              value={nextState}
            >
              <option value="">{t("field.keepState")}</option>
              {transitions.map((state) => (
                <option key={state} value={state}>{t(SANDBOX_STATE_LABEL_KEYS[state])}</option>
              ))}
            </select>
          </ConsoleField>
        ) : null}

        <ConsoleField hint={t("hint.autoStart")} label={t("field.autoStart")}>
          {/* One boolean, so the visible label is the field label above; the box
              carries the same string as its accessible name rather than repeating
              it on screen. */}
          <div className="sandbox-option-group">
            <label>
              <input
                aria-label={t("field.autoStart")}
                checked={draft.autoStart}
                disabled={busy}
                onChange={(event) => setDraft({ ...draft, autoStart: event.target.checked })}
                type="checkbox"
              />
            </label>
          </div>
        </ConsoleField>

        <ConsoleField
          error={fieldError("expiresAtLocal")}
          hint={t("hint.expiresAt")}
          htmlFor="sandbox-instance-expires-at"
          label={t("field.expiresAt")}
          optionalLabel={t("field.optional")}
        >
          <input
            id="sandbox-instance-expires-at"
            onChange={(event) => setDraft({ ...draft, expiresAtLocal: event.target.value })}
            type="datetime-local"
            value={draft.expiresAtLocal}
          />
        </ConsoleField>

        <ConsoleField
          hint={t("hint.workspaceId")}
          htmlFor="sandbox-instance-workspace"
          label={t("field.workspaceId")}
          optionalLabel={t("field.optional")}
        >
          <input
            id="sandbox-instance-workspace"
            onChange={(event) => setDraft({ ...draft, workspaceId: event.target.value })}
            placeholder={t("placeholder.workspaceId")}
            type="text"
            value={draft.workspaceId}
          />
        </ConsoleField>
      </ConsoleFormSection>

      <div className="sdkwork-surface-drawer-form-actions">
        <button
          className="sdkwork-surface-modal-cancel"
          disabled={busy}
          onClick={onCancel}
          type="button"
        >
          {t("dialog.cancel")}
        </button>
        <button className="skills-console-primary" disabled={busy} type="submit">
          {busy ? t("dialog.busy") : editing ? t("dialog.save") : t("dialog.create")}
        </button>
      </div>
    </form>
  );
}
