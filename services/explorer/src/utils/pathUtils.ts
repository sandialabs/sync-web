import { JournalPath } from '../types';

/**
 * Encode a JournalPath to a URL-safe string
 */
export const encodePathToHash = (path: JournalPath): string => {
  return encodeURIComponent(JSON.stringify(path));
};

/**
 * Decode a URL hash to a JournalPath
 */
export const decodeHashToPath = (hash: string): JournalPath | null => {
  if (!hash || hash === '#') return null;
  try {
    const decoded = decodeURIComponent(hash.startsWith('#') ? hash.slice(1) : hash);
    if (!decoded) return null;
    const parsed = JSON.parse(decoded);
    if (Array.isArray(parsed)) {
      return parsed as JournalPath;
    }
    return null;
  } catch {
    return null;
  }
};

export const stripLeadingIndex = (path: JournalPath): JournalPath =>
  typeof path[0] === 'number' ? path.slice(1) : [...path];

export const findLastMarkerIndex = (path: JournalPath, marker: string): number => {
  for (let i = path.length - 1; i >= 0; i--) {
    if (path[i] === marker) return i;
  }
  return -1;
};

export const isStagePath = (path: JournalPath): boolean => path[0] === '*state*';

/**
 * Generate expanded nodes set from a path.
 * This ensures all parent nodes are expanded so the selected path is visible.
 */
export const generateExpandedNodesFromPath = (path: JournalPath): Set<string> => {
  const expanded = new Set<string>();

  for (let i = 1; i <= path.length; i++) {
    expanded.add(JSON.stringify(path.slice(0, i)));
  }

  return expanded;
};

/**
 * Extract the marker/key segments without history indices, for comparison purposes.
 */
export const getBasePath = (path: JournalPath): string => {
  return JSON.stringify(path.filter(segment => typeof segment !== 'number'));
};

/**
 * Build a path with a specific version offset for a given tab index.
 */
export const buildVersionPath = (
  basePath: JournalPath,
  tabIndex: number,
  versionOffset: number
): JournalPath => {
  if (tabIndex === 0) {
    return typeof basePath[0] === 'number'
      ? [versionOffset, ...basePath.slice(1)]
      : [versionOffset, ...basePath];
  }

  const modifiedPath = [...basePath];
  let indexCount = 0;

  for (let i = 0; i < modifiedPath.length; i++) {
    if (typeof modifiedPath[i] === 'number') {
      indexCount++;
      if (indexCount === tabIndex + 1) {
        modifiedPath[i] = versionOffset;
        break;
      }
    }
  }

  return modifiedPath;
};

/**
 * Get the version index at a specific tab position in the path.
 */
export const getVersionAtTab = (path: JournalPath, tabIndex: number): number | null => {
  let indexCount = 0;
  for (const segment of path) {
    if (typeof segment === 'number') {
      if (indexCount === tabIndex) return segment;
      indexCount++;
    }
  }
  return null;
};

/**
 * Build the child path based on the parent node's path and the child item name.
 */
export const buildChildPath = (parentPath: JournalPath, itemName: string): JournalPath => {
  const last = parentPath[parentPath.length - 1];

  if (last === '*bridge*') {
    return [...parentPath, itemName, -1];
  }

  const stateIndex = findLastMarkerIndex(parentPath, '*state*');
  if (stateIndex >= 0) {
    return [...parentPath, itemName];
  }

  return parentPath;
};
