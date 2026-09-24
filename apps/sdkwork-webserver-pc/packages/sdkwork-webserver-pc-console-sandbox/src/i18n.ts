import type {
  SandboxInstanceProfile,
  SandboxInstanceState,
  SandboxIsolationAssurance,
  SandboxRuntimeCapability,
} from "@sdkwork/webserver-pc-console-core";

export type SandboxInstancesLocale = "en-US" | "zh-CN";
const enUs = {
  "title": "VM Instances",
  "description": "Virtual machine (VM) instances you provisioned — one isolated runtime each, sized and isolated the way you declared it.",
  "owner.scope": "Only the VM instances you own are listed here. Another account's instances are never shown on this page.",
  "refresh": "Refresh",
  "create": "Provision VM",
  "loading": "Loading VM instances…",
  "summary": "Showing {shown} of {total} VM instances",

  "column.name": "Name",
  "column.state": "State",
  "column.profile": "Profile",
  "column.resources": "Resources",
  "column.capabilities": "Required capabilities",
  "column.expiresAt": "Expires",
  "column.actions": "Actions",

  "state.requested": "Requested",
  "state.active": "Active",
  "state.suspended": "Suspended",
  "state.terminated": "Terminated",
  "state.failed": "Failed",

  "profile.standard": "Standard",
  "profile.memory_optimized": "Memory optimized",
  "profile.compute_optimized": "Compute optimized",

  "assurance.host_user": "Host user",
  "assurance.container": "Container",
  "assurance.user_space_kernel": "User-space kernel",
  "assurance.micro_vm": "Micro VM",
  "assurance.dedicated_vm": "Dedicated VM",

  "capability.terminal": "Terminal",
  "capability.filesystem": "Filesystem",
  "capability.git": "Git",
  "capability.build": "Build",
  "capability.browser": "Browser",
  "capability.port_forward": "Port forward",
  "capability.mcp_transport": "MCP transport",
  "capability.environment": "Environment",
  "capability.none": "Profile default",
  "capability.overflow": "+{count} more",

  "resources.summary": "{vcpu} vCPU · {memory} MiB · {disk} MiB",
  "expiresAt.never": "Never",
  "instances.neverExpires": "Never expires",
  "instances.noWorkspace": "Not attached to a workspace",
  "workspace.label": "Workspace",

  "action.edit": "Edit",
  "action.delete": "Delete",
  "action.deleting": "Deleting…",
  "action.changeState": "Change state",
  "action.moveTo": "Move to {state}",

  "filter.state": "Filter by state",
  "filter.allStates": "All states",
  "filter.clear": "Clear filter",
  "filter.empty.title": "No VM instance is in this state",
  "filter.empty.description": "Clear the filter to see every VM instance you own.",

  "empty.title": "No VM instances yet",
  "empty.description": "Provision one to give your agents an isolated runtime with the CPU, memory, and capabilities you specify.",
  "empty.action": "Provision VM",

  // A refused read must not borrow the empty wording above: "we could not list
  // them" and "you have none" are different facts with different fixes.
  "unavailable.description": "The request failed before the listing could be read. Retrying loads your VM instances again.",

  "drawer.createTitle": "Provision VM instance",
  "drawer.createDescription": "Size the runtime and declare how it must be isolated and what it must be able to do.",
  "drawer.editTitle": "Edit VM instance",
  "drawer.editDescription": "Editing “{name}”.",

  "section.shape": "Runtime shape",
  "section.isolation": "Isolation and capabilities",
  "section.lifecycle": "Lifecycle",

  "field.optional": "Optional",
  "field.name": "Name",
  "field.baseImage": "Base image",
  "field.profile": "Profile",
  "field.vcpuCount": "vCPU",
  "field.memoryMb": "Memory (MiB)",
  "field.diskMb": "Disk (MiB)",
  "field.assurance": "Minimum isolation assurance",
  "field.capabilities": "Required capabilities",
  "field.autoStart": "Start automatically",
  "field.expiresAt": "Expires at",
  "field.workspaceId": "Workspace ID",
  "field.nextState": "State change",
  "field.keepState": "Keep the current state",

  "hint.name": "Up to {max} characters. Must be unique among the VM instances you own.",
  "hint.baseImage": "The container image the runtime boots from.",
  "hint.baseImageImmutable": "The base image is fixed once the instance exists and is not sent on save.",
  "hint.profile": "The envelope the shape below is validated against.",
  "hint.vcpuCount": "{min}–{max} vCPU for the {profile} profile.",
  "hint.memoryMb": "{min}–{max} MiB for the {profile} profile.",
  "hint.diskMb": "{min}–{max} MiB for the {profile} profile.",
  "hint.assurance": "A host whose isolation is weaker than this cannot run the instance.",
  "hint.capabilities": "Up to {max} entries. Leave all unchecked to request the profile default.",
  "hint.autoStart": "Bring the runtime up as soon as provisioning completes.",
  "hint.expiresAt": "Leave empty for an instance that never expires. Times are entered in your local time zone and stored as UTC.",
  "hint.workspaceId": "Optional. Attach the VM to a workspace.",
  "hint.nextState": "Only the transitions this state accepts are offered. A terminal instance cannot be moved again.",

  "placeholder.name": "agent-vm-01",
  "placeholder.baseImage": "sdkwork/sandbox-runtime:latest",
  "placeholder.workspaceId": "ws_…",

  "validate.name.required": "Name is required.",
  "validate.name.tooLong": "Name must be at most {max} characters.",
  "validate.baseImage.required": "Base image is required.",
  "validate.baseImage.tooLong": "Base image must be at most {max} characters.",
  "validate.vcpu.notANumber": "Enter a whole number of vCPUs.",
  "validate.vcpu.outOfProfile": "vCPU must be between {min} and {max} for the {profile} profile.",
  "validate.memory.notANumber": "Enter a whole number of MiB.",
  "validate.memory.outOfProfile": "Memory must be between {min} and {max} MiB for the {profile} profile.",
  "validate.disk.notANumber": "Enter a whole number of MiB.",
  "validate.disk.outOfProfile": "Disk must be between {min} and {max} MiB for the {profile} profile.",
  "validate.expiresAt.invalid": "Enter a valid date and time.",
  "validate.unknown": "This value is not accepted.",

  "dialog.cancel": "Cancel",
  "dialog.create": "Provision",
  "dialog.save": "Save changes",
  "dialog.busy": "Working…",

  "delete.confirmTitle": "Delete VM instance?",
  "delete.confirmDescription": "Delete “{name}”? This removes the record and cannot be undone.",
  "delete.blockedHint": "A running instance cannot be deleted. Suspend or terminate it first.",

  "transition.confirmTitle": "Change state?",
  "transition.confirmDescription": "Move “{name}” from {from} to {to}? The runtime follows this declaration.",

  "error.load": "The VM instance list is unavailable.",
  "error.create": "The VM instance could not be provisioned.",
  "error.update": "The VM instance could not be updated.",
  "error.delete": "The VM instance could not be deleted.",
  "error.transition": "The state change was refused.",
} as const;

const zhCn: Record<keyof typeof enUs, string> = {
  "title": "虚拟机实例",
  "description": "你已开通的虚拟机实例——每台虚拟机都是一套独立运行时，规格与隔离等级按你的声明创建。",
  "owner.scope": "本页只列出你拥有的虚拟机实例，其他账号的实例不会出现在这里。",
  "refresh": "刷新",
  "create": "开通虚拟机",
  "loading": "正在加载虚拟机实例…",
  "summary": "显示 {shown} / {total} 个虚拟机实例",

  "column.name": "名称",
  "column.state": "状态",
  "column.profile": "规格档",
  "column.resources": "资源",
  "column.capabilities": "所需能力",
  "column.expiresAt": "到期时间",
  "column.actions": "操作",

  "state.requested": "已申请",
  "state.active": "运行中",
  "state.suspended": "已挂起",
  "state.terminated": "已终止",
  "state.failed": "失败",

  "profile.standard": "标准型",
  "profile.memory_optimized": "内存优化型",
  "profile.compute_optimized": "计算优化型",

  "assurance.host_user": "宿主用户级",
  "assurance.container": "容器级",
  "assurance.user_space_kernel": "用户态内核级",
  "assurance.micro_vm": "微型虚拟机级",
  "assurance.dedicated_vm": "独占虚拟机级",

  "capability.terminal": "终端",
  "capability.filesystem": "文件系统",
  "capability.git": "Git",
  "capability.build": "构建",
  "capability.browser": "浏览器",
  "capability.port_forward": "端口转发",
  "capability.mcp_transport": "MCP 传输",
  "capability.environment": "环境变量",
  "capability.none": "跟随规格档默认",
  "capability.overflow": "另 {count} 项",

  "resources.summary": "{vcpu} vCPU · {memory} MiB · {disk} MiB",
  "expiresAt.never": "永不过期",
  "instances.neverExpires": "永不过期",
  "instances.noWorkspace": "未关联工作区",
  "workspace.label": "工作区",

  "action.edit": "编辑",
  "action.delete": "删除",
  "action.deleting": "删除中…",
  "action.changeState": "变更状态",
  "action.moveTo": "变更为{state}",

  "filter.state": "按状态筛选",
  "filter.allStates": "全部状态",
  "filter.clear": "清除筛选",
  "filter.empty.title": "没有处于该状态的虚拟机实例",
  "filter.empty.description": "清除筛选即可查看你拥有的全部虚拟机实例。",

  "empty.title": "还没有虚拟机实例",
  "empty.description": "开通一台虚拟机，为你的 Agent 提供一套按需指定 CPU、内存与能力的独立运行时。",
  "empty.action": "开通虚拟机",

  // 读失败不能借用上面的空态文案：「没读到」和「你没有」是两个事实，修法也不同。
  "unavailable.description": "请求失败，列表未能读取。重试即可重新加载你的虚拟机实例。",

  "drawer.createTitle": "开通虚拟机实例",
  "drawer.createDescription": "设定运行时规格，并声明所需的隔离等级与能力。",
  "drawer.editTitle": "编辑虚拟机实例",
  "drawer.editDescription": "正在编辑“{name}”。",

  "section.shape": "运行时规格",
  "section.isolation": "隔离与能力",
  "section.lifecycle": "生命周期",

  "field.optional": "可选",
  "field.name": "名称",
  "field.baseImage": "基础镜像",
  "field.profile": "规格档",
  "field.vcpuCount": "vCPU",
  "field.memoryMb": "内存（MiB）",
  "field.diskMb": "磁盘（MiB）",
  "field.assurance": "最低隔离等级",
  "field.capabilities": "所需能力",
  "field.autoStart": "自动启动",
  "field.expiresAt": "到期时间",
  "field.workspaceId": "工作区 ID",
  "field.nextState": "状态变更",
  "field.keepState": "保持当前状态",

  "hint.name": "最多 {max} 个字符。同一账号下的虚拟机实例名称不可重复。",
  "hint.baseImage": "运行时启动所用的容器镜像。",
  "hint.baseImageImmutable": "实例创建后基础镜像即固定，保存时不会提交该字段。",
  "hint.profile": "下方规格将按该档位信封校验。",
  "hint.vcpuCount": "{profile}型为 {min}–{max} vCPU。",
  "hint.memoryMb": "{profile}型为 {min}–{max} MiB。",
  "hint.diskMb": "{profile}型为 {min}–{max} MiB。",
  "hint.assurance": "隔离等级弱于该要求的主机无法承载此实例。",
  "hint.capabilities": "最多 {max} 项。全部不勾选即按规格档默认。",
  "hint.autoStart": "开通完成后立即启动运行时。",
  "hint.expiresAt": "留空表示永不过期。此处按本地时区填写，存储为 UTC。",
  "hint.workspaceId": "可选。将该虚拟机关联到某个工作区。",
  "hint.nextState": "仅列出该状态允许的转移。终态实例无法再次变更。",

  "placeholder.name": "agent-vm-01",
  "placeholder.baseImage": "sdkwork/sandbox-runtime:latest",
  "placeholder.workspaceId": "ws_…",

  "validate.name.required": "请填写名称。",
  "validate.name.tooLong": "名称最多 {max} 个字符。",
  "validate.baseImage.required": "请填写基础镜像。",
  "validate.baseImage.tooLong": "基础镜像最多 {max} 个字符。",
  "validate.vcpu.notANumber": "请输入整数 vCPU 数量。",
  "validate.vcpu.outOfProfile": "{profile}型的 vCPU 需在 {min}–{max} 之间。",
  "validate.memory.notANumber": "请输入整数 MiB 数量。",
  "validate.memory.outOfProfile": "{profile}型的内存需在 {min}–{max} MiB 之间。",
  "validate.disk.notANumber": "请输入整数 MiB 数量。",
  "validate.disk.outOfProfile": "{profile}型的磁盘需在 {min}–{max} MiB 之间。",
  "validate.expiresAt.invalid": "请输入有效的日期与时间。",
  "validate.unknown": "该取值不被接受。",

  "dialog.cancel": "取消",
  "dialog.create": "开通",
  "dialog.save": "保存更改",
  "dialog.busy": "处理中…",

  "delete.confirmTitle": "删除虚拟机实例？",
  "delete.confirmDescription": "确认删除“{name}”？删除后记录不可恢复。",
  "delete.blockedHint": "运行中的实例不可删除，请先挂起或终止。",

  "transition.confirmTitle": "变更状态？",
  "transition.confirmDescription": "将“{name}”由{from}变更为{to}？运行时将按此声明调整。",

  "error.load": "虚拟机实例列表不可用。",
  "error.create": "虚拟机实例开通失败。",
  "error.update": "虚拟机实例更新失败。",
  "error.delete": "虚拟机实例删除失败。",
  "error.transition": "状态变更被拒绝。",
};

export type SandboxInstancesMessageKey = keyof typeof enUs;

/**
 * Runtime key list for the locale symmetry gate: every key must resolve in both
 * catalogs, so a newly added string cannot ship zh-less.
 */
export const SANDBOX_INSTANCES_MESSAGE_KEYS: Record<SandboxInstancesMessageKey, true> = Object.fromEntries(
  Object.keys(enUs).map((key) => [key, true]),
) as Record<SandboxInstancesMessageKey, true>;

/**
 * Validation codes the pure model emits (`validateSandboxInstanceDraft`)
 * mapped onto this catalog.
 *
 * The model returns bare codes — it has no locale — so the mapping has to live
 * somewhere that does. Keeping it an explicit registry rather than a template
 * literal (`validate.${code}`) is what makes a new server rule that was never
 * given copy show up as `validate.unknown` in the form instead of leaking a raw
 * identifier into the UI, and it keeps `translate` typed on the real key union.
 */
const VALIDATION_KEYS: Readonly<Record<string, SandboxInstancesMessageKey>> = {
  "baseImage.required": "validate.baseImage.required",
  "baseImage.tooLong": "validate.baseImage.tooLong",
  "disk.notANumber": "validate.disk.notANumber",
  "disk.outOfProfile": "validate.disk.outOfProfile",
  "expiresAt.invalid": "validate.expiresAt.invalid",
  "memory.notANumber": "validate.memory.notANumber",
  "memory.outOfProfile": "validate.memory.outOfProfile",
  "name.required": "validate.name.required",
  "name.tooLong": "validate.name.tooLong",
  "vcpu.notANumber": "validate.vcpu.notANumber",
  "vcpu.outOfProfile": "validate.vcpu.outOfProfile",
};

export function sandboxValidationMessageKey(code: string): SandboxInstancesMessageKey {
  return VALIDATION_KEYS[code] ?? "validate.unknown";
}

/**
 * Vocabulary -> message key, one exhaustive record per closed vocabulary from the
 * route contract.
 *
 * Exhaustive on purpose: the records are keyed by the transport's unions, so a
 * value added to the wire vocabulary is a compile error here rather than a state
 * or capability that renders as its raw identifier in the table.
 */
export const SANDBOX_STATE_LABEL_KEYS: Readonly<Record<SandboxInstanceState, SandboxInstancesMessageKey>> = {
  active: "state.active",
  failed: "state.failed",
  requested: "state.requested",
  suspended: "state.suspended",
  terminated: "state.terminated",
};

export const SANDBOX_PROFILE_LABEL_KEYS: Readonly<Record<SandboxInstanceProfile, SandboxInstancesMessageKey>> = {
  compute_optimized: "profile.compute_optimized",
  memory_optimized: "profile.memory_optimized",
  standard: "profile.standard",
};

export const SANDBOX_ASSURANCE_LABEL_KEYS: Readonly<Record<SandboxIsolationAssurance, SandboxInstancesMessageKey>> = {
  container: "assurance.container",
  dedicated_vm: "assurance.dedicated_vm",
  host_user: "assurance.host_user",
  micro_vm: "assurance.micro_vm",
  user_space_kernel: "assurance.user_space_kernel",
};

export const SANDBOX_CAPABILITY_LABEL_KEYS: Readonly<Record<SandboxRuntimeCapability, SandboxInstancesMessageKey>> = {
  browser: "capability.browser",
  build: "capability.build",
  environment: "capability.environment",
  filesystem: "capability.filesystem",
  git: "capability.git",
  mcp_transport: "capability.mcp_transport",
  port_forward: "capability.port_forward",
  terminal: "capability.terminal",
};

/**
 * The status tone a state paints with, keyed to the shared `.status-badge`
 * modifiers the console already styles.
 *
 * `suspended` and `terminated` get no modifier: neither is a success, a failure,
 * or something still pending, and inventing a tone for them would claim a
 * judgement the state does not carry. They render as the neutral pill.
 */
export const SANDBOX_STATE_TONE: Readonly<Record<SandboxInstanceState, string>> = {
  active: "status-active",
  failed: "status-failed",
  requested: "status-pending",
  suspended: "",
  terminated: "",
};

const catalogs: Record<SandboxInstancesLocale, Record<SandboxInstancesMessageKey, string>> = {
  "en-US": enUs,
  "zh-CN": zhCn,
};

export function normalizeSandboxInstancesLocale(locale?: string | null): SandboxInstancesLocale {
  if (!locale) return "en-US";
  const normalized = locale.trim().toLowerCase().replaceAll("_", "-");
  return normalized === "zh-cn" || normalized === "zh" || normalized.startsWith("zh-")
    ? "zh-CN"
    : "en-US";
}

export function translateSandboxInstances(
  locale: SandboxInstancesLocale,
  key: SandboxInstancesMessageKey,
  values: Record<string, string | number> = {},
): string {
  const template = catalogs[locale][key] ?? catalogs["en-US"][key] ?? String(key);
  return Object.entries(values).reduce(
    (message, [name, value]) => message.replaceAll(`{${name}}`, String(value)),
    template,
  );
}
