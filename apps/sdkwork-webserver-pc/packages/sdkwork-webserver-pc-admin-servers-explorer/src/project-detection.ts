import type { ServerEntry, ServerProjectType } from "./server-files-types.ts";

/**
 * Project type detection is the classifier that turns a flat directory listing
 * into a labeled project. The detection is deliberately declarative: add a
 * manifest marker below and the explorer picks it up without changing the UI.
 *
 * A directory is considered a project root when its own entries contain the
 * marker file(s). Directory entries that only *nest* another project (for
 * example a monorepo sub-folder) are classified but `isProjectRoot` stays
 * false unless the nested project marker is present in *this* listing.
 */

export const PROJECT_TYPE_MARKERS: ReadonlyArray<{
  type: ServerProjectType;
  label: string;
  markers: readonly string[];
  /** Order matters: earlier entries win when multiple markers collide. */
  priority: number;
}> = [
  {
    type: "flutter-app",
    label: "Flutter App",
    markers: ["pubspec.yaml"],
    priority: 90,
  },
  {
    type: "rust-backend",
    label: "Rust Backend",
    markers: ["Cargo.toml", "Cargo.lock"],
    priority: 85,
  },
  {
    type: "sdkwork-workspace",
    label: "SDKWork Workspace",
    markers: ["sdkwork.app.config.json", "sdkwork.workflow.json"],
    priority: 95,
  },
  {
    type: "h5-app",
    label: "H5 App",
    markers: ["vite.config.ts", "vite.config.js", "uni.scss", "manifest.json", "project.config.json"],
    priority: 70,
  },
  {
    type: "pc-app",
    label: "PC App",
    markers: ["vite.config.ts", "vite.config.js", "webpack.config.js", "tsconfig.json"],
    priority: 60,
  },
  {
    type: "node-backend",
    label: "Node Backend",
    markers: ["package.json"],
    priority: 40,
  },
];

const DIRECTORY_PROJECT_MARKERS: ReadonlyMap<string, ServerProjectType> = new Map([
  ["apps", "generic"],
  ["crates", "rust-backend"],
  ["packages", "generic"],
  ["sdks", "generic"],
  ["database", "generic"],
  ["deployments", "generic"],
]);

/** Project-type detection result for a single directory listing. */
export interface ProjectDetection {
  /** Classified project type, defaulting to `generic`. */
  type: ServerProjectType;
  /** Whether this directory is itself a project root (owns a manifest). */
  isProjectRoot: boolean;
  /** Human-readable label for the detected type. */
  label: string;
}

export function detectProjectType(entries: readonly ServerEntry[]): ProjectDetection {
  const names = new Set(entries.map((entry) => entry.name));
  const hasAny = (candidates: readonly string[]): boolean =>
    candidates.some((name) => names.has(name));

  const matched = PROJECT_TYPE_MARKERS.find((candidate) =>
    hasAny(candidate.markers),
  );
  if (matched) {
    return {
      type: matched.type,
      isProjectRoot: true,
      label: matched.label,
    };
  }

  // Fall back to conventional SDKWork monorepo directory shapes. These
  // indicate a nested workspace but not a standalone project root.
  for (const entry of entries) {
    if (entry.kind !== "directory") continue;
    const dirType = DIRECTORY_PROJECT_MARKERS.get(entry.name);
    if (dirType) {
      return { type: dirType, isProjectRoot: false, label: PROJECT_TYPE_LABEL[dirType] };
    }
  }

  return { type: "generic", isProjectRoot: false, label: PROJECT_TYPE_LABEL.generic };
}

export const PROJECT_TYPE_LABEL: Readonly<Record<ServerProjectType, string>> = {
  "h5-app": "H5 App",
  "pc-app": "PC App",
  "flutter-app": "Flutter App",
  "rust-backend": "Rust Backend",
  "node-backend": "Node Backend",
  "sdkwork-workspace": "SDKWork Workspace",
  generic: "Directory",
};

/**
 * Enrich a raw listing: attach projectType/isProjectRoot to every directory
 * entry based on the presence of manifest markers in that same listing.
 */
export function classifyListing(
  listing: { path: string; entries: ServerEntry[] },
): ServerEntry[] {
  const names = new Set(listing.entries.map((entry) => entry.name));
  return listing.entries.map((entry) => {
    if (entry.kind !== "directory") {
      return { ...entry, projectType: undefined, isProjectRoot: false };
    }
    const detection = detectProjectType(
      listing.entries.filter((candidate) => candidate.name === entry.name),
    );
    // A directory is a project root only when its own marker appears in the
    // *parent* listing (i.e. the child directory carries the manifest file).
    // In practice we detect the root on its own listing; here we conservatively
    // flag directories whose name matches a conventional project container.
    const isRoot = names.has(`${entry.name}`)
      && CONVENTIONAL_ROOT_SUFFIXES.some((suffix) => entry.name.endsWith(suffix))
      && detection.isProjectRoot;
    return {
      ...entry,
      projectType: detection.type,
      isProjectRoot: isRoot || detection.isProjectRoot,
    };
  });
}

const CONVENTIONAL_ROOT_SUFFIXES = ["-app", "-server", "-h5", "-pc"];
