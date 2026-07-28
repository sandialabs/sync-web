import React, { useEffect, useState } from 'react';
import { DirectoryEntry, DirectoryEntryType, ExplorerSelection, ExplorerMode, JournalPath, TreeNode } from '../types';
import { JournalService } from '../services/JournalService';
import { compareSegmentedNames } from '../utils/sortKeys';

const ACCESS_MESSAGE = 'Unable to load this location. Check that the selected journal has granted access to this user and path.';
const SNAPSHOT_MESSAGE = 'The selected ledger snapshot is unavailable. It may still be committing or may no longer be retained.';
const loadErrorMessage = (error: unknown): string =>
  JournalService.isSnapshotUnavailable(error) ? SNAPSHOT_MESSAGE : ACCESS_MESSAGE;

interface ExplorerTreeProps {
  mode: ExplorerMode;
  rootPath: JournalPath;
  selected: ExplorerSelection | null;
  expandedNodes: Set<string>;
  journalService: JournalService | null;
  currentUser?: string;
  refreshKey: number;
  onExpandedNodesChange: (expanded: Set<string>) => void;
  onSelect: (selection: ExplorerSelection) => void;
}

const buildStateChildPath = (parentPath: JournalPath, name: string): JournalPath => {
  if (!parentPath.includes('*state*')) {
    return parentPath;
  }
  return [...parentPath, name];
};

const createNode = (
  parentId: string,
  parentPath: JournalPath,
  entry: DirectoryEntry,
): TreeNode => ({
  id: `${parentId}/${entry.pathSegment ?? entry.name}`,
  label: entry.name,
  // A selectively disclosed ancestor proof can preserve a child name while
  // cutting its value/type. Keep unknown entries navigable; authorization is
  // checked again when the child is opened.
  type: entry.type === 'value' ? 'file' : 'directory',
  valueType: entry.type,
  path: buildStateChildPath(parentPath, entry.pathSegment ?? entry.name),
});

const compareDirectoryEntries = (
  left: { name: string; type: DirectoryEntryType },
  right: { name: string; type: DirectoryEntryType },
): number => {
  const leftRank = left.type === 'directory' ? 0 : 1;
  const rightRank = right.type === 'directory' ? 0 : 1;

  if (leftRank !== rightRank) {
    return leftRank - rightRank;
  }

  return compareSegmentedNames(left.name, right.name);
};

const ExplorerTree: React.FC<ExplorerTreeProps> = ({
  mode,
  rootPath,
  selected,
  expandedNodes,
  journalService,
  currentUser = '',
  refreshKey,
  onExpandedNodesChange,
  onSelect,
}) => {
  const [treeData, setTreeData] = useState<TreeNode[]>([]);
  const [loadingNodes, setLoadingNodes] = useState<Set<string>>(new Set());
  const [loadError, setLoadError] = useState<string | null>(null);
  const [nodeErrors, setNodeErrors] = useState<Record<string, string>>({});

  const loadDirectoryChildren = async (path: JournalPath, idPrefix: string): Promise<TreeNode[]> => {
    if (!journalService) {
      return [];
    }
    const entries = await journalService.getDirectoryEntries(path);
    const namespaceRoot = path[path.length - 1] === '*state*';
    return entries
      .filter((entry) => entry.name !== '*directory*')
      .sort((left, right) => {
        if (namespaceRoot && currentUser) {
          const leftIsCurrentUser = left.name === currentUser;
          const rightIsCurrentUser = right.name === currentUser;
          if (leftIsCurrentUser !== rightIsCurrentUser) {
            return leftIsCurrentUser ? -1 : 1;
          }
        }
        return compareDirectoryEntries(left, right);
      })
      .map((entry) => createNode(idPrefix, path, entry));
  };

  useEffect(() => {
    let active = true;

    const refreshedErrors: Record<string, string> = {};
    const hydrateExpandedChildren = async (nodes: TreeNode[]): Promise<TreeNode[]> =>
      Promise.all(
        nodes.map(async (node) => {
          if (node.type !== 'directory' || !expandedNodes.has(node.id)) {
            return node;
          }
          try {
            const children = await loadDirectoryChildren(node.path, node.id);
            return { ...node, children: await hydrateExpandedChildren(children) };
          } catch (error) {
            refreshedErrors[node.id] = loadErrorMessage(error);
            return node;
          }
        }),
      );

    const loadRoot = async () => {
      if (!journalService) {
        setTreeData([]);
        return;
      }

      setLoadError(null);
      try {
        const children = await hydrateExpandedChildren(await loadDirectoryChildren(rootPath, mode));
        if (active) {
          setTreeData(children);
          setNodeErrors(refreshedErrors);
          setLoadError(null);
        }
      } catch (error) {
        if (active) {
          setTreeData([]);
          setLoadError(loadErrorMessage(error));
        }
      }
    };

    loadRoot();
    return () => {
      active = false;
    };
  }, [currentUser, journalService, mode, refreshKey, rootPath]);

  const updateNodeChildren = (nodes: TreeNode[], nodeId: string, children: TreeNode[]): TreeNode[] =>
    nodes.map((node) => {
      if (node.id === nodeId) {
        return { ...node, children };
      }
      if (!node.children) {
        return node;
      }
      return { ...node, children: updateNodeChildren(node.children, nodeId, children) };
    });

  const toggleNode = async (node: TreeNode) => {
    const nextExpanded = new Set(expandedNodes);
    if (nextExpanded.has(node.id)) {
      nextExpanded.delete(node.id);
      onExpandedNodesChange(nextExpanded);
      return;
    }

    nextExpanded.add(node.id);
    onExpandedNodesChange(nextExpanded);

    if (node.children || !journalService || node.type !== 'directory') {
      return;
    }

    setLoadingNodes((prev) => new Set(prev).add(node.id));
    setNodeErrors((prev) => {
      const next = { ...prev };
      delete next[node.id];
      return next;
    });
    try {
      const children = await loadDirectoryChildren(node.path, node.id);
      setTreeData((prev) => updateNodeChildren(prev, node.id, children));
    } catch (error) {
      setNodeErrors((prev) => ({ ...prev, [node.id]: loadErrorMessage(error) }));
    } finally {
      setLoadingNodes((prev) => {
        const next = new Set(prev);
        next.delete(node.id);
        return next;
      });
    }
  };

  const selectedKey = selected ? JSON.stringify(selected.path) : null;

  const renderNode = (node: TreeNode, depth: number): JSX.Element => {
    const isExpanded = expandedNodes.has(node.id);
    const isSelected = selectedKey === JSON.stringify(node.path);
    const selectionType: ExplorerSelection['type'] = node.type === 'file' ? 'file' : 'directory';
    const isLoading = loadingNodes.has(node.id);
    const kindIcon = node.type === 'directory' ? '▣' : '▤';
    const nodeError = nodeErrors[node.id];
    const isCurrentUserRoot = depth === 0
      && rootPath[rootPath.length - 1] === '*state*'
      && node.label === currentUser;

    return (
      <div key={node.id} className="tree-node">
        <div
          className={`tree-node-content ${isSelected ? 'selected' : ''}`}
          style={{ paddingLeft: `${depth * 14}px` }}
        >
          <button
            className={`tree-node-icon ${node.type !== 'directory' ? 'disabled' : ''}`}
            onClick={() => node.type === 'directory' && toggleNode(node)}
            aria-label={isLoading ? `Loading ${node.label}` : undefined}
            aria-busy={isLoading || undefined}
            aria-describedby={nodeError ? `${node.id}-error` : undefined}
          >
            {isLoading ? '…' : node.type === 'directory' ? (isExpanded ? '▼' : '▶') : '•'}
          </button>
          <button
            className="tree-node-label"
            onClick={() => onSelect({ path: node.path, type: selectionType })}
          >
            <span className="tree-node-kind" aria-hidden="true">{kindIcon}</span>
            <span className={isCurrentUserRoot ? 'tree-node-current-user' : undefined}>
              {node.label}
            </span>
          </button>
        </div>
        {isExpanded && nodeError && (
          <div
            id={`${node.id}-error`}
            className="tree-node-error"
            role="alert"
            style={{ marginLeft: `${(depth + 1) * 14}px` }}
          >
            {nodeError}
          </div>
        )}
        {isExpanded && node.children && (
          <div className="tree-node-children">
            {node.children.map((child) => renderNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="tree-view">
      {loadError ? (
        <div className="tree-load-error" role="alert">{loadError}</div>
      ) : treeData.length === 0 ? (
        <div className="tree-empty-state">
          {mode === 'stage'
            ? 'No local documents yet.'
            : 'No documents available for this ledger route.'}
        </div>
      ) : (
        treeData.map((node) => renderNode(node, 0))
      )}
    </div>
  );
};

export default ExplorerTree;
