import type { SVGProps } from "react";

type PluginIconProps = SVGProps<SVGSVGElement> & { size?: number };

function PluginIcon({ size = 16, children, ...rest }: PluginIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {children}
    </svg>
  );
}

export function GitBranchIcon(props: PluginIconProps) {
  return (
    <PluginIcon {...props}>
      <line x1="6" y1="3" x2="6" y2="15" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </PluginIcon>
  );
}

export function ArchiveIcon(props: PluginIconProps) {
  return (
    <PluginIcon {...props}>
      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
      <path d="m3.3 7 8.7 5 8.7-5" />
      <path d="M12 22V12" />
    </PluginIcon>
  );
}

export function SearchIcon(props: PluginIconProps) {
  return (
    <PluginIcon {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </PluginIcon>
  );
}

export function CheckIcon(props: PluginIconProps) {
  return (
    <PluginIcon {...props}>
      <path d="m5 12 5 5L20 7" />
    </PluginIcon>
  );
}

export function UploadIcon(props: PluginIconProps) {
  return (
    <PluginIcon {...props}>
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <path d="m17 8-5-5-5 5" />
      <path d="M12 3v12" />
    </PluginIcon>
  );
}
