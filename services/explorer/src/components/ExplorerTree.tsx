import React, { useCallback, useEffect, useRef, useState } from 'react';
import { DirectoryEntry, DirectoryEntryType, ExplorerSelection, ExplorerMode, JournalPath, JournalPathSegment, TreeNode } from '../types';
import { JournalService } from '../services/JournalService';
import { compareSegmentedNames } from '../utils/sortKeys';
import { pathSegmentIdentity } from '../utils/pathUtils';

const ACCESS_MESSAGE = 'Unable to load this location. Check that the selected journal has granted access to this user and path.';
const SNAPSHOT_MESSAGE = 'The selected ledger snapshot is unavailable. It may still be committing or may no longer be retained.';
const loadErrorMessage = (error: unknown): string =>
  JournalService.isSnapshotUnavailable(error) ? SNAPSHOT_MESSAGE : ACCESS_MESSAGE;

interface ExplorerTreeProps {
  mode: ExplorerMode;
  rootPath: JournalPath;
  retainedRootPath?: JournalPath;
  selected: ExplorerSelection | null;
  expandedNodes: Set<string>;
  journalService: JournalService | null;
  requesterJournalService?: JournalService | null;
  currentUser?: string;
  refreshKey: number;
  onExpandedNodesChange: (expanded: Set<string>) => void;
  onSelect: (selection: ExplorerSelection) => void;
}

const buildStateChildPath = (parentPath: JournalPath, name: JournalPathSegment): JournalPath => {
  if (!parentPath.includes('*state*')) {
    return parentPath;
  }
  return [...parentPath, name];
};

const pathStartsWith = (path: JournalPath, prefix: JournalPath): boolean =>
  prefix.length <= path.length
  && prefix.every((segment, index) => JSON.stringify(segment) === JSON.stringify(path[index]));

const createNode = (
  parentId: string,
  parentPath: JournalPath,
  entry: DirectoryEntry,
): TreeNode => ({
  id: `${parentId}/${pathSegmentIdentity(entry.pathSegment ?? entry.name)}`,
  label: entry.name,
  // A selectively disclosed ancestor proof can preserve a child name while
  // cutting its value/type. Keep unknown entries navigable; authorization is
  // checked again when the child is opened.
  type: entry.type === 'value' ? 'file' : entry.type === 'object' ? 'object' : 'directory',
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
  retainedRootPath = rootPath,
  selected,
  expandedNodes,
  journalService,
  requesterJournalService = journalService,
  currentUser = '',
  refreshKey,
  onExpandedNodesChange,
  onSelect,
}) => {
  const [treeData, setTreeData] = useState<TreeNode[]>([]);
  const [loadingNodes, setLoadingNodes] = useState<Set<string>>(new Set());
  const [loadError, setLoadError] = useState<string | null>(null);
  const [nodeErrors, setNodeErrors] = useState<Record<string, string>>({});
  const requestContext = JSON.stringify({ mode, rootPath, retainedRootPath, refreshKey });
  const requestContextRef = useRef(requestContext);
  requestContextRef.current = requestContext;

  const loadStateChildren = useCallback(async (path: JournalPath, idPrefix: string): Promise<TreeNode[]> => {
    if (!journalService) return [];
    const requesterStateRoot: JournalPath = [...rootPath, '*state*'];
    const service = mode === 'ledger' && requesterJournalService
      && pathStartsWith(path, requesterStateRoot)
      ? requesterJournalService : journalService;
    const entries = await service.getDirectoryEntries(path);
    const namespaceRoot = path[path.length - 1] === '*state*';
    return entries
      .sort((left, right) => {
        if (namespaceRoot && currentUser) {
          const leftIsCurrentUser = left.name === currentUser;
          const rightIsCurrentUser = right.name === currentUser;
          if (leftIsCurrentUser !== rightIsCurrentUser) return leftIsCurrentUser ? -1 : 1;
        }
        return compareDirectoryEntries(left, right);
      })
      .map((entry) => createNode(idPrefix, path, entry));
  }, [currentUser, journalService, mode, requesterJournalService, rootPath]);

  const indexHasAccessibleChild = useCallback(async (indexPath: JournalPath): Promise<boolean> => {
    if (!journalService) return false;
    try {
      if ((await journalService.getDirectoryEntries([...indexPath, '*state*'])).length > 0) {
        return true;
      }
    } catch {
      // An inaccessible State child does not admit the retained index.
    }
    try {
      return (await journalService.getDirectoryEntries([...indexPath, '*bridge*'])).length > 0;
    } catch {
      return false;
    }
  }, [journalService]);

  const loadAdmittedIndexes = useCallback(async (
    bridgePath: JournalPath,
    idPrefix: string,
    indexes: number[],
  ): Promise<TreeNode[]> => {
    const nodes: TreeNode[] = [];
    for (const index of indexes) {
      const path = [...bridgePath, index];
      if (await indexHasAccessibleChild(path)) {
        nodes.push({
          id: `${idPrefix}/${index}`,
          label: String(index),
          type: 'directory',
          path,
          navigationKind: 'index',
          childrenLoaded: false,
        });
      }
    }
    return nodes;
  }, [indexHasAccessibleChild]);

  const loadBridgeChildren = useCallback(async (path: JournalPath, idPrefix: string): Promise<TreeNode[]> => {
    if (!journalService) return [];
    const entries = (await journalService.getDirectoryEntries(path)).sort(compareDirectoryEntries);
    const nodes: TreeNode[] = [];
    for (const entry of entries) {
      const bridgePath = [...path, entry.pathSegment ?? entry.name];
      const id = `${idPrefix}/${pathSegmentIdentity(entry.pathSegment ?? entry.name)}`;
      let children: TreeNode[] = [];
      try {
        const inventory = await journalService.getChainInventory(bridgePath);
        children = await loadAdmittedIndexes(bridgePath, id, inventory.indexes);
      } catch {
        // The authenticated bridge entry remains selectable without implying indexes.
      }
      nodes.push({
        id,
        label: entry.name,
        type: 'directory',
        path: bridgePath,
        navigationKind: 'bridge',
        children,
        childrenLoaded: true,
      });
    }
    return nodes;
  }, [journalService, loadAdmittedIndexes]);

  const loadNodeChildren = useCallback(async (node: TreeNode): Promise<TreeNode[]> => {
    if (!journalService) return [];
    if (node.navigationKind === 'bridges') {
      return loadBridgeChildren(node.path, node.id);
    }
    if (node.navigationKind === 'bridge') {
      const inventory = await journalService.getChainInventory(node.path);
      return loadAdmittedIndexes(node.path, node.id, inventory.indexes);
    }
    if (node.navigationKind === 'index') {
      const children: TreeNode[] = [];
      const admittedState = node.children?.find((child) => child.navigationKind === 'state');
      if (admittedState) {
        children.push(admittedState);
      } else {
        const statePath: JournalPath = [...node.path, '*state*'];
        try {
          const state: TreeNode = {
            id: `${node.id}/state`, label: 'State', type: 'directory', path: statePath,
            navigationKind: 'state',
            children: await loadStateChildren(statePath, `${node.id}/state`),
          };
          children.push(state);
        } catch {
          // Unauthorized or unavailable retained state is omitted from navigation.
        }
      }
      const bridgesPath: JournalPath = [...node.path, '*bridge*'];
      try {
        const bridgeChildren = await loadBridgeChildren(bridgesPath, `${node.id}/bridges`);
        if (bridgeChildren.length > 0) {
          children.push({
            id: `${node.id}/bridges`, label: 'Bridges', type: 'directory',
            path: bridgesPath, navigationKind: 'bridges', children: bridgeChildren,
          });
        }
      } catch {
        // Sparse or unauthorized nested bridge structure stays hidden.
      }
      return children;
    }
    return loadStateChildren(node.path, node.id);
  }, [journalService, loadAdmittedIndexes, loadBridgeChildren, loadStateChildren]);

  useEffect(() => {
    let active = true;

    const refreshedErrors: Record<string, string> = {};
    const hydrateExpandedChildren = async (nodes: TreeNode[]): Promise<TreeNode[]> =>
      Promise.all(
        nodes.map(async (node) => {
          const autoExpanded = mode === 'ledger' && node.navigationKind === 'state'
            && node.id === 'ledger/state';
          if (node.type !== 'directory' || (!expandedNodes.has(node.id) && !autoExpanded)) return node;
          try {
            const children = node.children && node.childrenLoaded !== false
              ? node.children
              : await loadNodeChildren(node);
            return {
              ...node,
              children: await hydrateExpandedChildren(children),
              childrenLoaded: true,
            };
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
        const roots: TreeNode[] = mode === 'ledger'
          ? [
              {
                id: 'ledger/state', label: 'State', type: 'directory',
                path: [...rootPath, '*state*'], navigationKind: 'state',
              },
              {
                id: 'ledger/bridges', label: 'Bridges', type: 'directory',
                path: [...retainedRootPath, '*bridge*'], navigationKind: 'bridges',
              },
            ]
          : await loadStateChildren(rootPath, mode);
        const children = await hydrateExpandedChildren(roots);
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
  }, [
    expandedNodes, journalService, loadNodeChildren, loadStateChildren, mode,
    refreshKey, retainedRootPath, rootPath,
  ]);

  const updateNodeChildren = (nodes: TreeNode[], nodeId: string, children: TreeNode[]): TreeNode[] =>
    nodes.map((node) => {
      if (node.id === nodeId) {
        return { ...node, children, childrenLoaded: true };
      }
      if (!node.children) {
        return node;
      }
      return { ...node, children: updateNodeChildren(node.children, nodeId, children) };
    });

  const toggleNode = async (node: TreeNode) => {
    const context = requestContextRef.current;
    const nextExpanded = new Set(expandedNodes);
    if (nextExpanded.has(node.id)) {
      nextExpanded.delete(node.id);
      onExpandedNodesChange(nextExpanded);
      return;
    }

    nextExpanded.add(node.id);
    onExpandedNodesChange(nextExpanded);

    if ((node.children && node.childrenLoaded !== false)
      || !journalService || node.type !== 'directory') {
      return;
    }

    setLoadingNodes((prev) => new Set(prev).add(node.id));
    setNodeErrors((prev) => {
      const next = { ...prev };
      delete next[node.id];
      return next;
    });
    try {
      const children = await loadNodeChildren(node);
      if (context !== requestContextRef.current) return;
      setTreeData((prev) => updateNodeChildren(prev, node.id, children));
    } catch (error) {
      if (context !== requestContextRef.current) return;
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
    const isLedgerStateRoot = mode === 'ledger' && depth === 0
      && node.navigationKind === 'state';
    const isExpandable = node.type === 'directory'
      && (node.childrenLoaded !== true || (node.children?.length ?? 0) > 0);
    const isExpanded = isExpandable && (expandedNodes.has(node.id) || isLedgerStateRoot);
    const isSelected = selectedKey === JSON.stringify(node.path);
    const selectionType: ExplorerSelection['type'] = node.type === 'object'
      ? 'object' : node.type === 'file' ? 'file' : 'directory';
    const isLoading = loadingNodes.has(node.id);
    const kindIcon = node.type === 'directory' ? '▣' : node.type === 'object' ? '◆' : '▤';
    const nodeError = nodeErrors[node.id];
    const isCurrentUserRoot = depth === 1
      && rootPath[rootPath.length - 1] === '*state*'
      && node.label === currentUser;

    return (
      <div key={node.id} className={`tree-node ${isLedgerStateRoot ? 'tree-root-node' : ''}`}>
        <div
          className={`tree-node-content ${isSelected ? 'selected' : ''}`}
          style={{ paddingLeft: `${depth * 14}px` }}
        >
          <button
            className={`tree-node-icon ${!isExpandable ? 'disabled' : ''}`}
            onClick={() => isExpandable && toggleNode(node)}
            aria-label={isLoading ? `Loading ${node.label}` : undefined}
            aria-busy={isLoading || undefined}
            aria-describedby={nodeError ? `${node.id}-error` : undefined}
          >
            {isLoading ? '…' : isExpandable ? (isExpanded ? '▼' : '▶') : '•'}
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
            {node.children.length > 0
              ? node.children.map((child) => renderNode(child, depth + 1))
              : node.navigationKind === 'index' && (
                <div className="tree-node-empty">No accessible contents</div>
              )}
          </div>
        )}
      </div>
    );
  };

  const rootSelected = selectedKey === JSON.stringify(rootPath);

  if (mode === 'ledger') {
    return (
      <div className="tree-view retained-tree">
        {loadError ? (
          <div className="tree-load-error" role="alert">{loadError}</div>
        ) : treeData.length === 0 ? (
          <div className="tree-empty-state">No retained state is available.</div>
        ) : (
          treeData.map((node) => renderNode(node, 0))
        )}
      </div>
    );
  }

  return (
    <div className="tree-view">
      <div className="tree-node tree-root-node">
        <div className={`tree-node-content ${rootSelected ? 'selected' : ''}`}>
          <button
            className="tree-node-icon disabled"
            aria-hidden="true"
            tabIndex={-1}
          >
            ▼
          </button>
          <button
            className="tree-node-label"
            onClick={() => onSelect({ path: rootPath, type: 'directory' })}
          >
            <span className="tree-node-kind" aria-hidden="true">▣</span>
            <span>State</span>
          </button>
        </div>
        <div className="tree-node-children">
          {loadError ? (
            <div className="tree-load-error" role="alert">{loadError}</div>
          ) : treeData.length === 0 ? (
            <div className="tree-empty-state">
              {mode === 'stage'
                ? 'No local documents yet.'
                : 'No documents available for this ledger route.'}
            </div>
          ) : (
            treeData.map((node) => renderNode(node, 1))
          )}
        </div>
      </div>
    </div>
  );
};

export default ExplorerTree;
