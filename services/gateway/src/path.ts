import { isAbsolute, relative, sep } from "node:path";

export const isPathWithinDirectory = (directory: string, candidate: string): boolean => {
  const relPath = relative(directory, candidate);
  const escapesDirectory = relPath === ".." || relPath.startsWith(`..${sep}`);
  return relPath === "" || (!escapesDirectory && !isAbsolute(relPath));
};
