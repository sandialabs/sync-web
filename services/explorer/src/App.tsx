import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import './App.css';
import ToolBar from './components/ToolBar';
import ExplorerTree from './components/ExplorerTree';
import ExplorerContent from './components/ExplorerContent';
import LedgerRouteBar from './components/LedgerRouteBar';
import WorkingRouteBar from './components/WorkingRouteBar';
import AccessPanel from './components/AccessPanel';
import AdminPanel from './components/AdminPanel';
import { GatewayChangeEvent, JournalService } from './services/JournalService';
import { AppState, ExplorerMode, ExplorerSelection, FederationContext, JournalPath, LedgerHop } from './types';
import {
  LEDGER_LATEST,
  buildLedgerStateRootPath,
  normalizeSnapshotInput,
  stepSnapshotValue,
} from './utils/ledgerRoute';
import {
  buildFragmentHash,
  getInitialLedgerHops,
  parseFragmentHash,
} from './utils/projectedFragments';

const getEnvVar = (key: string): string => {
  // @ts-ignore
  if (window._env_?.[key]) {
    // @ts-ignore
    return window._env_[key];
  }
  return process.env[`REACT_APP_${key}`] || '';
};

const getInitialTheme = (): 'light' | 'dark' => {
  const stored = localStorage.getItem('theme');
  if (stored === 'light' || stored === 'dark') {
    return stored;
  }
  if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    return 'dark';
  }
  return 'light';
};

const createInitialAppState = (): AppState => ({
  endpoint: getEnvVar('SYNC_EXPLORER_ENDPOINT'),
  rootIndex: -1,
  selectedPath: null,
  expandedNodes: new Set(),
  isLoading: false,
  error: null,
});

const STAGE_ROOT_PATH: JournalPath = ['*state*'];

const buildUserHomePath = (username: string): JournalPath => (
  username ? ['*state*', username] : STAGE_ROOT_PATH
);

const buildHopsForWorkingRoute = (previous: LedgerHop[], route: string[]): LedgerHop[] => [
  {
    key: 'local-0',
    kind: 'local',
    name: 'Self',
    snapshot: previous[0]?.snapshot ?? LEDGER_LATEST,
  },
  ...route.map((name, index) => ({
    key: `${name}-${index + 1}`,
    kind: 'bridge' as const,
    name,
    snapshot: previous[index + 1]?.name === name
      ? previous[index + 1].snapshot
      : LEDGER_LATEST,
  })),
];

const buildFederationContext = (
  route: string[],
  hops: LedgerHop[],
  rootIndex: number,
): FederationContext => {
  const routeHops = [hops[0], ...route.map((name, index) => {
    const hop = hops[index + 1];
    return hop?.name === name ? hop : undefined;
  })];
  const historyIndexes = routeHops.map((hop, index) => {
    if (!hop || hop.snapshot.trim().toLowerCase() === LEDGER_LATEST) return -1;
    const parsed = Number.parseInt(hop.snapshot, 10);
    return Number.isNaN(parsed) ? (index === 0 ? rootIndex : -1) : parsed;
  });
  return {
    route,
    ...(route.length > 0 ? { historyIndexes } : {}),
  };
};

const rebaseLedgerSelection = (
  selection: ExplorerSelection | null,
  hops: LedgerHop[],
  rootIndex: number,
): ExplorerSelection | null => {
  if (!selection) return null;
  const stateIndex = selection.path.lastIndexOf('*state*');
  if (stateIndex < 0) return null;
  return {
    ...selection,
    path: [
      ...buildLedgerStateRootPath(hops, rootIndex),
      ...selection.path.slice(stateIndex + 1),
    ],
  };
};

export const accessibleRemoteSelection = async (
  service: Pick<JournalService, 'get'>,
  selection: ExplorerSelection | null,
  fallbackPath: JournalPath,
): Promise<ExplorerSelection> => {
  if (selection) {
    try {
      await service.get(selection.path, { pinned: false, proof: false });
      return selection;
    } catch {
      // A local selection can be valid at Self but unauthorized at the target.
    }
  }

  const fallback: ExplorerSelection = { path: fallbackPath, type: 'directory' };
  await service.get(fallback.path, { pinned: false, proof: false });
  return fallback;
};

export const terminalRootSelections = async (
  service: Pick<JournalService, 'get' | 'setFederationContext'>,
  route: string[],
  hops: LedgerHop[],
  rootIndex: number,
): Promise<{ stage: ExplorerSelection; ledger: ExplorerSelection }> => {
  const stage: ExplorerSelection = { path: STAGE_ROOT_PATH, type: 'directory' };
  const ledger: ExplorerSelection = {
    path: buildLedgerStateRootPath(hops, rootIndex),
    type: 'directory',
  };

  service.setFederationContext(buildFederationContext(route, hops, rootIndex));
  await service.get(stage.path, { pinned: false, proof: false });
  await service.get(ledger.path, { pinned: false, proof: false });
  return { stage, ledger };
};

export const isStageNamespaceRoot = (selection: ExplorerSelection | null): boolean =>
  selection?.path.length === 1 && selection.path[0] === '*state*';

const isPathWithin = (candidate: JournalPath, ancestor: JournalPath): boolean => {
  if (ancestor.length > candidate.length) {
    return false;
  }

  return ancestor.every((segment, index) => JSON.stringify(segment) === JSON.stringify(candidate[index]));
};

const buildStageChildPath = (parentPath: JournalPath, childName: string): JournalPath => {
  if (parentPath[0] !== '*state*') {
    throw new Error('Expected a stage state path');
  }
  return [...parentPath, JournalService.encodePathSegment(childName)];
};

const buildStageSiblingPath = (path: JournalPath, siblingName: string): JournalPath => {
  if (path[0] !== '*state*') {
    throw new Error('Expected a stage state path');
  }
  return [...path.slice(0, -1), JournalService.encodePathSegment(siblingName)];
};

const stagePathToTreeNodeId = (path: JournalPath): string => {
  if (path[0] !== '*state*') {
    throw new Error('Expected a stage state path');
  }

  const suffix = path.slice(1);
  return suffix.length > 0 ? `stage/${suffix.join('/')}` : 'stage';
};

export const treeAncestorNodeIds = (
  mode: 'stage' | 'ledger',
  rootPath: JournalPath,
  selectedPath: JournalPath,
): Set<string> => {
  if (!isPathWithin(selectedPath, rootPath)) {
    return new Set();
  }

  const suffix = selectedPath.slice(rootPath.length);
  const ancestors = new Set<string>();
  for (let length = 1; length < suffix.length; length += 1) {
    ancestors.add(`${mode}/${suffix.slice(0, length).join('/')}`);
  }
  return ancestors;
};

const replaceStagePathPrefix = (
  candidate: JournalPath,
  sourcePrefix: JournalPath,
  targetPrefix: JournalPath,
): JournalPath => {
  if (!isPathWithin(candidate, sourcePrefix)) {
    return candidate;
  }

  return [
    ...targetPrefix,
    ...candidate.slice(sourcePrefix.length),
  ];
};

const App: React.FC = () => {
  const [sessionStatus, setSessionStatus] = useState<'checking' | 'ready' | 'unauthenticated' | 'error'>('checking');
  const [sessionName, setSessionName] = useState<string>('');
  const [appState, setAppState] = useState<AppState>(createInitialAppState);
  const [theme, setTheme] = useState<'light' | 'dark'>(getInitialTheme);
  const [mode, setMode] = useState<ExplorerMode>('ledger');
  const [journalService, setJournalService] = useState<JournalService | null>(null);
  const [adminStatus, setAdminStatus] = useState<'checking' | 'admin' | 'not-admin'>('checking');
  const [stageSelection, setStageSelection] = useState<ExplorerSelection | null>(null);
  const [ledgerSelection, setLedgerSelection] = useState<ExplorerSelection | null>(null);
  const [stageExpandedNodes, setStageExpandedNodes] = useState<Set<string>>(new Set());
  const [ledgerExpandedNodes, setLedgerExpandedNodes] = useState<Set<string>>(new Set());
  const [workingRoute, setWorkingRoute] = useState<string[]>([]);
  const [workingPeerChoices, setWorkingPeerChoices] = useState<string[] | null>(null);
  const [ledgerHops, setLedgerHops] = useState<LedgerHop[]>(getInitialLedgerHops(-1));
  const [ledgerView, setLedgerView] = useState<'content' | 'proof'>('content');
  const [stageRefreshKey, setStageRefreshKey] = useState(0);
  const [ledgerRefreshKey, setLedgerRefreshKey] = useState(0);
  const [adminRefreshKey, setAdminRefreshKey] = useState(0);
  const [accessRefreshKey, setAccessRefreshKey] = useState(0);
  const isApplyingHashRef = useRef(false);
  const federationContextRef = useRef('');
  const eventRefreshTimerRef = useRef<number | null>(null);
  const navigationTransitionRef = useRef(0);
  const ledgerHopsRef = useRef<LedgerHop[]>(getInitialLedgerHops(-1));

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
  }, [theme]);

  useEffect(() => {
    fetch('/auth/.ory/sessions/whoami')
      .then(async (res) => {
        if (res.ok) {
          const data = await res.json();
          const username = data?.identity?.traits?.username ?? '';
          setSessionName(username);
          if (appState.endpoint) {
            const service = new JournalService(appState.endpoint);
            setJournalService(service);
            if (username) {
              void service.ensureDirectory(buildUserHomePath(username)).catch(() => {
                // Home creation is best-effort here; normal Explorer requests surface
                // actionable authorization/gateway errors if the session is unusable.
              });
            }
          } else {
            setAdminStatus('not-admin');
          }
          setSessionStatus('ready');
        } else {
          setSessionStatus('unauthenticated');
        }
      })
      .catch(() => {
        setSessionStatus('error');
      });
  }, [appState.endpoint]);

  const setLoadingState = (isLoading: boolean, error: string | null = null) => {
    setAppState((prev) => ({ ...prev, isLoading, error }));
  };

  useLayoutEffect(() => {
    ledgerHopsRef.current = ledgerHops;
    if (!journalService) return;
    const context = buildFederationContext(workingRoute, ledgerHops, appState.rootIndex);
    journalService.setFederationContext(context);

    // A parsed deep link can mount the tree before its route/history context
    // reaches the mutable service. Reload once after each actual context
    // transition so a stale ownerless local read cannot leave an empty routed
    // tree mounted until the user changes paths.
    const contextKey = JSON.stringify(context);
    if (federationContextRef.current !== contextKey) {
      federationContextRef.current = contextKey;
      setLedgerRefreshKey((prev) => prev + 1);
    }
  }, [journalService, workingRoute, ledgerHops, appState.rootIndex]);

  useEffect(() => {
    if (!journalService) {
      return;
    }

    let cancelled = false;
    setAdminStatus('checking');
    journalService.getAdmins()
      .then(() => {
        if (!cancelled) {
          setAdminStatus('admin');
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAdminStatus('not-admin');
        }
      });

    return () => {
      cancelled = true;
    };
  }, [journalService, workingRoute]);

  useEffect(() => {
    if (adminStatus === 'not-admin' && mode === 'admin') {
      setMode('ledger');
    }
  }, [adminStatus, mode]);

  const synchronizeLedger = useCallback(async (options: { quiet?: boolean; preserveDeepLink?: boolean } = {}) => {
    if (!journalService) {
      return;
    }
    const transition = ++navigationTransitionRef.current;

    if (!options.quiet) {
      setLoadingState(true, null);
    }
    try {
      const size = await journalService.getSize();
      const latestIndex = Math.max(0, size - 1);
      const updatedHops = ledgerHops.map((hop, index) =>
        index === 0 && !options.preserveDeepLink ? { ...hop, snapshot: LEDGER_LATEST } : hop,
      );
      let nextSelection = ledgerSelection;
      if (!options.preserveDeepLink) {
        const rootPath = buildLedgerStateRootPath(updatedHops, latestIndex);
        nextSelection = await accessibleRemoteSelection(
          journalService,
          rebaseLedgerSelection(ledgerSelection, updatedHops, latestIndex),
          rootPath,
        );
      }
      if (transition !== navigationTransitionRef.current) return;
      setAppState((prev) => ({
        ...prev,
        rootIndex: latestIndex,
        isLoading: options.quiet ? prev.isLoading : false,
        error: null,
      }));
      if (!options.preserveDeepLink) {
        setLedgerHops(updatedHops);
        setLedgerSelection(nextSelection);
      }
      setLedgerRefreshKey((prev) => prev + 1);
    } catch (error) {
      if (!options.quiet && transition === navigationTransitionRef.current) {
        setAppState((prev) => ({
          ...prev,
          isLoading: false,
          error: `Synchronization failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
        }));
      }
    }
  }, [journalService, ledgerHops, ledgerSelection]);

  useEffect(() => {
    if (journalService && appState.rootIndex < 0) {
      const parsed = parseFragmentHash(window.location.hash);
      void synchronizeLedger({
        preserveDeepLink: parsed?.mode === 'ledger'
          || (parsed?.mode === 'stage' && 'ledgerHops' in parsed),
      });
    }
  }, [journalService, appState.rootIndex, synchronizeLedger]);

  useEffect(() => {
    if (!journalService || sessionStatus !== 'ready') {
      return;
    }

    const scheduleRefresh = (event: GatewayChangeEvent) => {
      if (eventRefreshTimerRef.current !== null) {
        window.clearTimeout(eventRefreshTimerRef.current);
      }
      eventRefreshTimerRef.current = window.setTimeout(() => {
        eventRefreshTimerRef.current = null;
        if (mode === 'admin') {
          setAdminRefreshKey((prev) => prev + 1);
          return;
        }
        if (mode === 'access') {
          setAccessRefreshKey((prev) => prev + 1);
          return;
        }
        if (mode === 'stage') {
          setStageRefreshKey((prev) => prev + 1);
          return;
        }
        setLedgerRefreshKey((prev) => prev + 1);
      }, 300);
    };

    const unsubscribe = journalService.subscribeEvents({
      onChange: scheduleRefresh,
      onError: () => {
        // EventSource reconnects automatically. Keep the UI quiet; normal requests still
        // surface actionable errors when refreshes fail.
      },
    });

    return () => {
      unsubscribe();
      if (eventRefreshTimerRef.current !== null) {
        window.clearTimeout(eventRefreshTimerRef.current);
        eventRefreshTimerRef.current = null;
      }
    };
  }, [journalService, sessionStatus, mode, synchronizeLedger]);

  const ledgerRootPath = useMemo(
    () => buildLedgerStateRootPath(
      ledgerHops,
      appState.rootIndex >= 0 ? appState.rootIndex : 0,
    ),
    [ledgerHops, appState.rootIndex],
  );
  const stageRootPath = STAGE_ROOT_PATH;
  useEffect(() => {
    const applyHash = () => {
      const parsed = parseFragmentHash(window.location.hash);
      if (!parsed) {
        return;
      }

      isApplyingHashRef.current = true;
      setMode(parsed.mode);
      setAppState((prev) => ({ ...prev, error: null }));
      navigationTransitionRef.current += 1;

      if (parsed.mode === 'stage') {
        const parsedHops = 'ledgerHops' in parsed ? parsed.ledgerHops : undefined;
        const nextHops = parsedHops
          ?? [ledgerHopsRef.current[0] ?? getInitialLedgerHops(-1)[0]];
        const routeChanged = JSON.stringify(ledgerHopsRef.current.map((hop) => [
          hop.kind,
          hop.name,
          hop.snapshot,
        ])) !== JSON.stringify(nextHops.map((hop) => [
          hop.kind,
          hop.name,
          hop.snapshot,
        ]));
        const nextStageSelection = parsed.selection
          ?? { path: STAGE_ROOT_PATH, type: 'directory' as const };
        ledgerHopsRef.current = nextHops;
        setWorkingRoute(nextHops.slice(1).map((hop) => hop.name));
        setLedgerHops(nextHops);
        if (routeChanged) {
          setLedgerSelection(null);
          setLedgerExpandedNodes(new Set());
          setStageExpandedNodes(treeAncestorNodeIds(
            'stage',
            STAGE_ROOT_PATH,
            nextStageSelection.path,
          ));
          setStageRefreshKey((prev) => prev + 1);
        }
        setStageSelection(nextStageSelection);
      } else if (parsed.mode === 'ledger') {
        const parsedHops = parsed.ledgerHops ?? getInitialLedgerHops(-1);
        ledgerHopsRef.current = parsedHops;
        setWorkingRoute(parsedHops.slice(1).map((hop) => hop.name));
        setLedgerHops(parsedHops);
        setLedgerSelection(parsed.selection);
        setLedgerView('content');
      }

      window.setTimeout(() => {
        isApplyingHashRef.current = false;
      }, 0);
    };

    applyHash();
    window.addEventListener('hashchange', applyHash);
    return () => window.removeEventListener('hashchange', applyHash);
  }, []);

  const navigateWorkingRoute = async (nextRoute: string[], nextMode?: ExplorerMode) => {
    if (!journalService) return;
    const transition = ++navigationTransitionRef.current;
    const nextHops = buildHopsForWorkingRoute(ledgerHops, nextRoute);
    const nextRootIndex = appState.rootIndex >= 0 ? appState.rootIndex : 0;
    let roots: { stage: ExplorerSelection; ledger: ExplorerSelection };
    try {
      roots = await terminalRootSelections(
        new JournalService(appState.endpoint), nextRoute, nextHops, nextRootIndex,
      );
    } catch (error) {
      if (transition === navigationTransitionRef.current) {
        setAppState((prev) => ({
          ...prev,
          error: `Selected route is not accessible: ${error instanceof Error ? error.message : 'Unknown error'}`,
        }));
      }
      return;
    }
    if (transition !== navigationTransitionRef.current) return;
    setWorkingRoute(nextRoute);
    setLedgerHops(nextHops);
    setWorkingPeerChoices(null);
    setStageSelection(roots.stage);
    setLedgerSelection(roots.ledger);
    setStageExpandedNodes(new Set());
    setLedgerExpandedNodes(new Set());
    if (nextMode) setMode(nextMode);
    if (nextMode === 'ledger') setLedgerView('content');
    setAppState((prev) => ({ ...prev, error: null }));
  };

  const handleModeChange = (nextMode: ExplorerMode) => {
    if (nextMode === 'stage' || nextMode === 'ledger') {
      setMode(nextMode);
      if (nextMode === 'stage') {
        setStageSelection((prev) => prev ?? { path: stageRootPath, type: 'directory' });
      } else {
        setLedgerSelection((prev) => prev ?? { path: ledgerRootPath, type: 'directory' });
        setLedgerView('content');
      }
      setAppState((prev) => ({ ...prev, error: null }));
      return;
    }
    if (workingRoute.length > 0 && (nextMode === 'access' || nextMode === 'admin')) {
      return;
    }
    if (nextMode === 'admin' && adminStatus !== 'admin') {
      return;
    }
    setMode(nextMode);
    if (nextMode === 'admin') setAdminRefreshKey((prev) => prev + 1);
    setAppState((prev) => ({ ...prev, error: null }));
  };

  const handleRenameStageNode = async (path: JournalPath, label: string) => {
    if (!journalService) {
      return;
    }
    const nextName = window.prompt('Rename to:', label);
    if (!nextName || nextName.trim() === '' || nextName === label) {
      return;
    }

    try {
      const renamedPath = buildStageSiblingPath(path, nextName.trim());
      await journalService.renameStagePath(path, nextName.trim());
      setStageSelection((prev) => {
        if (!prev || !isPathWithin(prev.path, path)) {
          return prev;
        }
        return {
          ...prev,
          path: replaceStagePathPrefix(prev.path, path, renamedPath),
        };
      });
      setStageExpandedNodes((prev) => {
        const sourceId = stagePathToTreeNodeId(path);
        const targetId = stagePathToTreeNodeId(renamedPath);
        const next = new Set<string>();
        prev.forEach((id) => {
          if (id === sourceId || id.startsWith(`${sourceId}/`)) {
            next.add(targetId + id.slice(sourceId.length));
          } else {
            next.add(id);
          }
        });
        return next;
      });
      setStageRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, error: null }));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Rename failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const handleDeleteStageNode = async (path: JournalPath, label: string) => {
    if (!journalService) {
      return;
    }
    const confirmed = window.confirm(`Delete ${label}?`);
    if (!confirmed) {
      return;
    }

    try {
      await journalService.deleteStagePath(path);
      setStageSelection((prev) =>
        prev && isPathWithin(prev.path, path) ? null : prev,
      );
      setStageExpandedNodes((prev) => {
        const sourceId = stagePathToTreeNodeId(path);
        const next = new Set<string>();
        prev.forEach((id) => {
          if (id !== sourceId && !id.startsWith(`${sourceId}/`)) {
            next.add(id);
          }
        });
        return next;
      });
      setStageRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, error: null }));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Delete failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const promptForName = (message: string): string | null => {
    const value = window.prompt(message);
    if (!value || value.trim() === '') {
      return null;
    }
    return value.trim();
  };

  const handleStageCreateFile = async (path: JournalPath) => {
    if (!journalService) {
      return;
    }
    const name = promptForName('Enter document name:');
    if (!name) {
      return;
    }
    try {
      const createdPath = buildStageChildPath(path, name);
      await journalService.createFile(path, name);
      setStageSelection({
        path: createdPath,
        type: 'file',
      });
      setStageExpandedNodes((prev) => {
        const next = new Set(prev);
        next.add(stagePathToTreeNodeId(path));
        next.add(stagePathToTreeNodeId(createdPath));
        return next;
      });
      setStageRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, error: null }));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Create document failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const handleStageCreateDirectory = async (path: JournalPath) => {
    if (!journalService) {
      return;
    }
    const name = promptForName('Enter folder name:');
    if (!name) {
      return;
    }
    try {
      const createdPath = buildStageChildPath(path, name);
      await journalService.createDirectory(path, name);
      setStageSelection({
        path: createdPath,
        type: 'directory',
      });
      setStageExpandedNodes((prev) => {
        const next = new Set(prev);
        next.add(stagePathToTreeNodeId(path));
        next.add(stagePathToTreeNodeId(createdPath));
        return next;
      });
      setStageRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, error: null }));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Create folder failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const handleStageUploadFile = async (path: JournalPath, file: File) => {
    if (!journalService) {
      return;
    }
    try {
      await journalService.uploadFile(path, file);
      const uploadedPath = buildStageChildPath(path, file.name);
      setStageSelection({
        path: uploadedPath,
        type: 'file',
      });
      setStageExpandedNodes((prev) => {
        const next = new Set(prev);
        next.add(stagePathToTreeNodeId(path));
        next.add(stagePathToTreeNodeId(uploadedPath));
        return next;
      });
      setStageRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, error: null }));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Upload document failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const applyLedgerHops = async (nextHops: LedgerHop[]) => {
    if (!journalService) return;
    const transition = ++navigationTransitionRef.current;
    const rootIndex = appState.rootIndex >= 0 ? appState.rootIndex : 0;
    const rootPath = buildLedgerStateRootPath(nextHops, rootIndex);
    try {
      const nextSelection = await accessibleRemoteSelection(
        journalService,
        rebaseLedgerSelection(ledgerSelection, nextHops, rootIndex),
        rootPath,
      );
      if (transition !== navigationTransitionRef.current) return;
      setLedgerHops(nextHops);
      setLedgerSelection(nextSelection);
      setLedgerRefreshKey((prev) => prev + 1);
      setAppState((prev) => ({ ...prev, isLoading: false, error: null }));
    } catch (error) {
      if (transition === navigationTransitionRef.current) {
        setAppState((prev) => ({
          ...prev,
          isLoading: false,
          error: `Ledger snapshot is not accessible: ${error instanceof Error ? error.message : 'Unknown error'}`,
        }));
      }
    }
  };

  const handleLedgerSnapshotChange = (index: number, value: string) => {
    const nextHops = ledgerHops.map((hop, hopIndex) =>
      hopIndex === index ? { ...hop, snapshot: value } : hop,
    );
    void applyLedgerHops(nextHops);
  };

  const handleLedgerStepSnapshot = (index: number, direction: 'older' | 'newer') => {
    const nextHops = ledgerHops.map((hop, hopIndex) => {
      if (hopIndex !== index) return hop;
      if (index > 0) {
        return { ...hop, snapshot: stepSnapshotValue(hop.snapshot, direction) };
      }

      const current = hop.snapshot.trim().toLowerCase() === LEDGER_LATEST
        ? appState.rootIndex
        : Number.parseInt(hop.snapshot, 10);
      const safeCurrent = Number.isNaN(current) ? appState.rootIndex : current;
      if (direction === 'older') {
        return { ...hop, snapshot: String(Math.max(0, safeCurrent - 1)) };
      }
      return safeCurrent + 1 >= appState.rootIndex
        ? { ...hop, snapshot: LEDGER_LATEST }
        : { ...hop, snapshot: String(safeCurrent + 1) };
    });
    void applyLedgerHops(nextHops);
  };

  const handleOpenWorkingPeerPicker = async () => {
    if (!journalService) return;
    try {
      const peers = await journalService.getBridges(
        workingRoute.length > 0 ? 'working' : undefined,
      );
      setWorkingPeerChoices(peers.map((peer) => peer.name));
    } catch (error) {
      setAppState((prev) => ({
        ...prev,
        error: `Bridge lookup failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const handleChooseWorkingPeer = async (peerName: string) => {
    const transition = ++navigationTransitionRef.current;
    let originIndex = appState.rootIndex;
    const originLatest = ledgerHops[0]?.snapshot.trim().toLowerCase() === LEDGER_LATEST;
    if (originLatest) {
      if (!journalService) return;
      try {
        originIndex = Math.max(0, (await journalService.getLocalSize()) - 1);
        if (transition !== navigationTransitionRef.current) return;
        setAppState((prev) => ({ ...prev, rootIndex: originIndex, error: null }));
      } catch (error) {
        if (transition === navigationTransitionRef.current) {
          setAppState((prev) => ({
            ...prev,
            error: `Synchronization failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
          }));
        }
        return;
      }
    }

    const nextRoute = [...workingRoute, peerName];
    const nextHops = buildHopsForWorkingRoute(ledgerHops, nextRoute);
    const nextRootIndex = originIndex >= 0 ? originIndex : 0;
    if (!journalService) return;
    let roots: { stage: ExplorerSelection; ledger: ExplorerSelection };
    try {
      roots = await terminalRootSelections(
        new JournalService(appState.endpoint), nextRoute, nextHops, nextRootIndex,
      );
    } catch (error) {
      if (transition === navigationTransitionRef.current) {
        setAppState((prev) => ({
          ...prev,
          error: `Remote route is not accessible: ${error instanceof Error ? error.message : 'Unknown error'}`,
        }));
      }
      return;
    }
    if (transition !== navigationTransitionRef.current) return;
    setWorkingRoute(nextRoute);
    setLedgerHops(nextHops);
    setWorkingPeerChoices(null);
    setStageSelection(roots.stage);
    setLedgerSelection(roots.ledger);
    setStageExpandedNodes(new Set());
    setLedgerExpandedNodes(new Set());
    setAppState((prev) => ({ ...prev, error: null }));
  };

  const handleRemoveWorkingHop = () => {
    void navigateWorkingRoute(workingRoute.slice(0, -1));
  };

  const handleSelectWorkingHop = (index: number) => {
    void navigateWorkingRoute(workingRoute.slice(0, index));
  };

  useEffect(() => {
    if (workingRoute.length > 0 && (mode === 'access' || mode === 'admin')) {
      setMode('ledger');
    }
  }, [workingRoute, mode]);

  useEffect(() => {
    setLedgerHops((prev) =>
      prev.map((hop, index) => {
        if (index === 0) {
          return hop;
        }

        return { ...hop, snapshot: normalizeSnapshotInput(hop.snapshot) };
      }),
    );
  }, [appState.rootIndex]);

  useEffect(() => {
    setLedgerView('content');
  }, [ledgerSelection]);

  useEffect(() => {
    if (isApplyingHashRef.current || (mode === 'ledger' && appState.rootIndex < 0)) {
      return;
    }

    const nextHash = buildFragmentHash({
      mode,
      stageSelection,
      ledgerSelection,
      ledgerRootPath,
      ledgerHops,
      rootIndex: appState.rootIndex,
    });

    if (window.location.hash !== nextHash) {
      window.history.replaceState(
        null,
        '',
        `${window.location.pathname}${window.location.search}${nextHash}`,
      );
    }
  }, [mode, stageSelection, ledgerSelection, ledgerRootPath, ledgerHops, appState.rootIndex]);

  const handleContentSelection = (selection: ExplorerSelection) => {
    const contentMode = mode === 'stage' ? 'stage' : 'ledger';
    const rootPath = contentMode === 'stage' ? stageRootPath : ledgerRootPath;
    const ancestors = treeAncestorNodeIds(contentMode, rootPath, selection.path);
    const mergeAncestors = (expanded: Set<string>): Set<string> => {
      const next = new Set(expanded);
      ancestors.forEach((ancestor) => next.add(ancestor));
      return next;
    };

    if (mode === 'stage') {
      setStageSelection(selection);
      setStageExpandedNodes(mergeAncestors);
      if (ancestors.size > 0) setStageRefreshKey((prev) => prev + 1);
    } else {
      setLedgerSelection(selection);
      setLedgerExpandedNodes(mergeAncestors);
      if (ancestors.size > 0) setLedgerRefreshKey((prev) => prev + 1);
    }
  };

  if (sessionStatus === 'checking') {
    return (
      <div className="app session-checking">
        <span className="session-spinner" aria-label="Checking session…" />
      </div>
    );
  }

  if (sessionStatus === 'unauthenticated' || sessionStatus === 'error') {
    const signInUrl = '/auth/login?return_to=' + encodeURIComponent(window.location.href);
    return (
      <div className="app">
        <ToolBar
          sessionName=""
          error={sessionStatus === 'error' ? 'Could not check session' : null}
          mode={mode}
          isAdmin={false}
          theme={theme}
          onModeChange={handleModeChange}
          onThemeToggle={() => setTheme((prev) => (prev === 'light' ? 'dark' : 'light'))}
        />

        <div className="session-required">
          <section className="session-required-panel">
            <div className="session-required-kicker">Authentication</div>
            <h1>Sign in to use Explorer</h1>
            <p>
              Explorer needs a Synchronic session before it can read or write journal state.
            </p>
            <div className="session-required-actions">
              <a className="button button-primary" href={signInUrl}>Sign in</a>
              {sessionStatus === 'error' && (
                <button className="button button-secondary" onClick={() => window.location.reload()}>
                  Retry
                </button>
              )}
            </div>
          </section>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <ToolBar
        sessionName={sessionName}
        error={appState.error}
        mode={mode}
        isAdmin={adminStatus === 'admin'}
        localOnlyDisabled={workingRoute.length > 0}
        theme={theme}
        onModeChange={handleModeChange}
        onThemeToggle={() => setTheme((prev) => (prev === 'light' ? 'dark' : 'light'))}
      />

      <WorkingRouteBar
        route={workingRoute}
        peerChoices={workingPeerChoices}
        onRemoveHop={handleRemoveWorkingHop}
        onSelectHop={handleSelectWorkingHop}
        onOpenPeerPicker={handleOpenWorkingPeerPicker}
        onClosePeerPicker={() => setWorkingPeerChoices(null)}
        onChoosePeer={handleChooseWorkingPeer}
      />

      {mode === 'ledger' && (
        <LedgerRouteBar
          hops={ledgerHops}
          rootIndex={appState.rootIndex}
          onSynchronize={synchronizeLedger}
          isSynchronizing={appState.isLoading}
          onSnapshotChange={handleLedgerSnapshotChange}
          onStepSnapshot={handleLedgerStepSnapshot}
          readOnlyRoute
        />
      )}

      {mode === 'access' ? (
        journalService ? (
          <div className="main-content">
            <AccessPanel
              journalService={journalService}
              currentUser={sessionName}
              refreshKey={accessRefreshKey}
              isAdmin={adminStatus === 'admin'}
            />
          </div>
        ) : null
      ) : mode === 'admin' ? (
        adminStatus === 'admin' && journalService ? (
        <div className="main-content">
          <AdminPanel
            journalService={journalService}
            currentUser={sessionName}
            refreshKey={adminRefreshKey}
          />
        </div>
        ) : (
          <div className="main-content">
            <div className="admin-panel">
              <div className="admin-loading">Checking admin access...</div>
            </div>
          </div>
        )
      ) : (
      <div className="main-content two-pane">
        <div className="left-pane pane">
          <ExplorerTree
            mode={mode}
            rootPath={mode === 'stage' ? stageRootPath : ledgerRootPath}
            selected={mode === 'stage' ? stageSelection : ledgerSelection}
            expandedNodes={mode === 'stage' ? stageExpandedNodes : ledgerExpandedNodes}
            journalService={journalService}
            currentUser={sessionName}
            refreshKey={mode === 'stage' ? stageRefreshKey : ledgerRefreshKey}
            onExpandedNodesChange={mode === 'stage' ? setStageExpandedNodes : setLedgerExpandedNodes}
            onSelect={(selection) => {
              if (mode === 'stage') {
                setStageSelection(selection);
              } else {
                setLedgerSelection(selection);
              }
            }}
          />
        </div>

        <div className="middle-pane pane">
          <ExplorerContent
            mode={mode}
            selection={mode === 'stage' ? stageSelection : ledgerSelection}
            journalService={journalService}
            refreshKey={mode === 'stage' ? stageRefreshKey : ledgerRefreshKey}
            ledgerView={ledgerView}
            stageReadOnly={mode === 'stage' && isStageNamespaceRoot(stageSelection)}
            onLedgerViewToggle={() =>
              setLedgerView((prev) => (prev === 'content' ? 'proof' : 'content'))
            }
            onStageCreateFile={handleStageCreateFile}
            onStageCreateDirectory={handleStageCreateDirectory}
            onStageUploadFile={handleStageUploadFile}
            onStageRename={handleRenameStageNode}
            onStageDelete={handleDeleteStageNode}
            onSelectPath={handleContentSelection}
          />
        </div>
      </div>
      )}
    </div>
  );
};

export default App;
