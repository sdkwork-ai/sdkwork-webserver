import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Tailwind-aware class-name joiner shared by every H5 presentation primitive. */
export function cx(...values: ClassValue[]): string {
  return twMerge(clsx(values));
}
