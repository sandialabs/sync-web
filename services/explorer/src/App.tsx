import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import './App.css';
import ToolBar from './components/ToolBar';
import ExplorerTree from './components/ExplorerTree';
import ExplorerContent from './components/ExplorerContent';
import PutResourceModal from './components/PutResourceModal';
import LedgerRouteBar from './components/LedgerRouteBar';
import WorkingRouteBar from './components/WorkingRouteBar';
import AccessPanel from './components/AccessPanel';
import AdminPanel from './components/AdminPanel';
import { GatewayChangeEvent, JournalService, ResourcePutInput } from './services/JournalService';
import { AppState, ExplorerMode, ExplorerSelection, FederationContext, JournalPath, LedgerHop } from './types';
import {
  LEDGER_LATEST,
  buildLedgerRouteBasePath,
  normalizeSnapshotInput,
  retainedLedgerRootPath,
  stepSnapshotValue,
} from './utils/ledgerRoute';
import {
  buildFragmentHash,
  getInitialLedgerHops,
  parseFragmentHash,
} from './utils/projectedFragments';
import { pathSegmentIdentity } from './utils/pathUtils';

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
    ...previous[0],
    key: 'local-0',
    kind: 'local',
    name: 'Self',
    snapshot: previous[0]?.snapshot ?? LEDGER_LATEST,
  },
  ...route.map((name, index) => {
    const retained = previous[index + 1]?.name === name
      ? previous[index + 1]
      : null;
    return {
      ...retained,
      key: `${name}-${index + 1}`,
      kind: 'bridge' as const,
      name,
      snapshot: retained?.snapshot ?? LEDGER_LATEST,
    };
  }),
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

export const resolveLedgerHopIndexes = async (
  endpoint: string,
  hops: LedgerHop[],
  refreshAll: boolean,
): Promise<LedgerHop[]> => {
  const resolved = hops.map((hop) => ({ ...hop }));
  const local = new JournalService(endpoint);
  if (refreshAll || resolved[0]?.maximum === undefined
      || resolved[0]?.snapshot.trim().toLowerCase() === LEDGER_LATEST) {
    const maximum = Math.max(0, (await local.getLocalSize()) - 1);
    resolved[0] = {
      ...resolved[0],
      snapshot: normalizeSnapshotInput(resolved[0]?.snapshot ?? String(maximum), maximum),
      maximum,
    };
  }

  for (let index = 1; index < resolved.length; index += 1) {
    if (!refreshAll && resolved[index].maximum !== undefined
        && resolved[index].snapshot.trim().toLowerCase() !== LEDGER_LATEST) continue;
    const route = resolved.slice(1, index + 1).map((hop) => hop.name);
    const probeHops = resolved.slice(0, index + 1);
    probeHops[index] = { ...probeHops[index], snapshot: LEDGER_LATEST };
    const provider = new JournalService(endpoint);
    provider.setFederationContext(buildFederationContext(route, probeHops, Number(resolved[0].snapshot)));
    const maximum = Math.max(0, (await provider.getSize()) - 1);
    resolved[index] = {
      ...resolved[index],
      snapshot: normalizeSnapshotInput(resolved[index].snapshot, maximum),
      maximum,
    };
  }
  return resolved;
};

const rebaseLedgerSelection = (
  selection: ExplorerSelection | null,
  _hops: LedgerHop[],
  rootIndex: number,
): ExplorerSelection | null => {
  if (!selection) return null;
  const stateIndex = selection.path.lastIndexOf('*state*');
  if (stateIndex < 0) return null;
  return {
    ...selection,
    path: [rootIndex, '*state*', ...selection.path.slice(stateIndex + 1)],
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
  providerService: Pick<JournalService, 'get' | 'setFederationContext'>,
  requesterService: Pick<JournalService, 'get'>,
  route: string[],
  hops: LedgerHop[],
  rootIndex: number,
): Promise<{ stage: ExplorerSelection; ledger: ExplorerSelection }> => {
  const stage: ExplorerSelection = { path: STAGE_ROOT_PATH, type: 'directory' };
  const ledger: ExplorerSelection = {
    path: [rootIndex, '*state*'],
    type: 'directory',
  };

  providerService.setFederationContext(buildFederationContext(route, hops, rootIndex));
  const providerAdmission = providerService.get(stage.path, { pinned: false, proof: false });
  const requesterAdmission = requesterService.get(ledger.path, { pinned: false, proof: false })
    .then(
      () => ({ ok: true as const }),
      (error: unknown) => ({ ok: false as const, error }),
    );
  await providerAdmission;
  const requesterResult = await requesterAdmission;
  if (!requesterResult.ok) throw requesterResult.error;
  return { stage, ledger };
};

export const isStageNamespaceRoot = (selection: ExplorerSelection | null): boolean =>
  selection?.path.length === 1 && selection.path[0] === '*state*';

const isJournalPath = (value: unknown): value is JournalPath => Array.isArray(value)
  && value.every((segment) => {
    if (typeof segment === 'string') return true;
    if (typeof segment === 'number') return Number.isFinite(segment) && Number.isInteger(segment);
    if (typeof segment !== 'object' || segment === null || Array.isArray(segment)) return false;
    const keys = Object.keys(segment);
    return keys.length === 1
      && keys[0] === '*type/string*'
      && typeof (segment as Record<string, unknown>)['*type/string*'] === 'string';
  });

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
  return suffix.length > 0
    ? `stage/${suffix.map(pathSegmentIdentity).join('/')}`
    : 'stage';
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
  if (mode === 'ledger') {
    const ids: string[] = ['ledger'];
    suffix.forEach((segment, index) => {
      if (index === 0 && segment === '*state*') ids.push('state');
      else if (index === 0 && typeof segment !== 'number' && segment !== '*bridge*') {
        ids.push('bridges', pathSegmentIdentity(segment));
      } else if (segment === '*bridge*') ids.push('bridges');
      else if (segment === '*state*') ids.push('state');
      else if (typeof segment === 'number') ids.push(String(segment));
      else ids.push(pathSegmentIdentity(segment));
      if (index < suffix.length - 1) ancestors.add(ids.join('/'));
    });
    return ancestors;
  }
  for (let length = 1; length < suffix.length; length += 1) {
    ancestors.add(`${mode}/${suffix.slice(0, length).map(pathSegmentIdentity).join('/')}`);
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
  const [mode, setMode] = useState<ExplorerMode>('stage');
  const [journalService, setJournalService] = useState<JournalService | null>(null);
  const [adminStatus, setAdminStatus] = useState<'checking' | 'admin' | 'not-admin'>('checking');
  const [stageSelection, setStageSelection] = useState<ExplorerSelection | null>({
    path: STAGE_ROOT_PATH, type: 'directory',
  });
  const [ledgerSelection, setLedgerSelection] = useState<ExplorerSelection | null>(null);
  const [stageExpandedNodes, setStageExpandedNodes] = useState<Set<string>>(new Set());
  const [ledgerExpandedNodes, setLedgerExpandedNodes] = useState<Set<string>>(new Set());
  const [workingRoute, setWorkingRoute] = useState<string[]>([]);
  const [workingPeerChoices, setWorkingPeerChoices] = useState<string[] | null>(null);
  const [ledgerHops, setLedgerHops] = useState<LedgerHop[]>(getInitialLedgerHops(-1));
  const [ledgerView, setLedgerView] = useState<'content' | 'proof'>('content');
  const [stageRefreshKey, setStageRefreshKey] = useState(0);
  const [stageTreeRefreshKey, setStageTreeRefreshKey] = useState(0);
  const [ledgerRefreshKey, setLedgerRefreshKey] = useState(0);
  const [adminRefreshKey, setAdminRefreshKey] = useState(0);
  const [accessRefreshKey, setAccessRefreshKey] = useState(0);
  const [putParentPath, setPutParentPath] = useState<JournalPath | null>(null);
  const isApplyingHashRef = useRef(false);
  const federationContextRef = useRef('');
  const eventRefreshTimerRef = useRef<number | null>(null);
  const stageContentRefreshPendingRef = useRef(false);
  const stageSelectionRef = useRef(stageSelection);
  const navigationTransitionRef = useRef(0);
  const peerPickerRequestRef = useRef(0);
  const ledgerHopsRef = useRef<LedgerHop[]>(getInitialLedgerHops(-1));

  const closeWorkingPeerPicker = useCallback(() => {
    peerPickerRequestRef.current += 1;
    setWorkingPeerChoices(null);
  }, []);

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
    stageSelectionRef.current = stageSelection;
  }, [stageSelection]);

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
  }, [journalService]);

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
      const updatedHops = await resolveLedgerHopIndexes(
        appState.endpoint, ledgerHops, !options.preserveDeepLink,
      );
      const latestIndex = Number.parseInt(updatedHops[0]?.snapshot ?? '0', 10);
      let nextSelection = ledgerSelection;
      if (!options.preserveDeepLink) {
        const rootPath: JournalPath = [latestIndex, '*state*'];
        nextSelection = await accessibleRemoteSelection(
          new JournalService(appState.endpoint),
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
      setLedgerHops(updatedHops);
      if (!options.preserveDeepLink) setLedgerSelection(nextSelection);
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
  }, [journalService, ledgerHops, ledgerSelection, appState.endpoint]);

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
      if (mode === 'stage') {
        const eventPath = isJournalPath(event.path) ? event.path : null;
        const selectionPath = stageSelectionRef.current?.path;
        if (!eventPath || !selectionPath
            || isPathWithin(eventPath, selectionPath)
            || isPathWithin(selectionPath, eventPath)) {
          stageContentRefreshPendingRef.current = true;
        }
      }
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
          setStageTreeRefreshKey((prev) => prev + 1);
          if (stageContentRefreshPendingRef.current) {
            stageContentRefreshPendingRef.current = false;
            setStageRefreshKey((prev) => prev + 1);
          }
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
      stageContentRefreshPendingRef.current = false;
    };
  }, [journalService, sessionStatus, mode, synchronizeLedger]);

  const ledgerRootPath = useMemo(
    () => buildLedgerRouteBasePath(ledgerHops.slice(0, 1), appState.rootIndex),
    [ledgerHops, appState.rootIndex],
  );
  const retainedRootPath = useMemo(
    () => retainedLedgerRootPath(ledgerHops, appState.rootIndex),
    [ledgerHops, appState.rootIndex],
  );
  const requesterJournalService = useMemo(
    () => journalService ? new JournalService(appState.endpoint) : null,
    [journalService, appState.endpoint],
  );
  const ledgerReadPath = useMemo<JournalPath | null>(() => {
    if (!ledgerSelection) return null;
    const requesterStateRoot: JournalPath = [...ledgerRootPath, '*state*'];
    if (ledgerHops.length <= 1 || !isPathWithin(ledgerSelection.path, requesterStateRoot)) {
      return ledgerSelection.path;
    }
    return [...retainedRootPath, ...ledgerSelection.path.slice(ledgerRootPath.length)];
  }, [ledgerHops, ledgerRootPath, ledgerSelection, retainedRootPath]);
  const contentJournalService = useMemo(() => {
    if (mode !== 'ledger' || !ledgerSelection || !requesterJournalService) {
      return journalService;
    }
    const requesterStateRoot: JournalPath = [...ledgerRootPath, '*state*'];
    return isPathWithin(ledgerSelection.path, requesterStateRoot) && ledgerHops.length <= 1
      ? requesterJournalService : journalService;
  }, [journalService, ledgerHops, ledgerRootPath, ledgerSelection, mode, requesterJournalService]);
  const stageRootPath = STAGE_ROOT_PATH;
  useEffect(() => {
    const applyHash = () => {
      const parsed = parseFragmentHash(window.location.hash);
      if (!parsed) {
        return;
      }

      isApplyingHashRef.current = true;
      closeWorkingPeerPicker();
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
  }, [closeWorkingPeerPicker]);

  const navigateWorkingRoute = async (nextRoute: string[], nextMode?: ExplorerMode) => {
    closeWorkingPeerPicker();
    if (!journalService || !requesterJournalService) return;
    const transition = ++navigationTransitionRef.current;
    const nextHops = buildHopsForWorkingRoute(ledgerHops, nextRoute);
    const nextRootIndex = Number.parseInt(
      nextHops[0]?.snapshot ?? String(appState.rootIndex), 10,
    );
    let roots: { stage: ExplorerSelection; ledger: ExplorerSelection };
    try {
      roots = await terminalRootSelections(
        new JournalService(appState.endpoint), requesterJournalService,
        nextRoute, nextHops, nextRootIndex,
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
    if (nextMode === 'admin' && adminStatus !== 'admin') return;
    closeWorkingPeerPicker();
    const transition = ++navigationTransitionRef.current;
    setWorkingRoute([]);
    setStageExpandedNodes(new Set());
    setLedgerExpandedNodes(new Set());
    setLedgerView('content');

    if (nextMode === 'stage') {
      const local = getInitialLedgerHops(appState.rootIndex);
      if (appState.rootIndex >= 0) {
        local[0] = { ...local[0], snapshot: String(appState.rootIndex), maximum: appState.rootIndex };
      }
      setLedgerHops(local);
      setStageSelection({ path: STAGE_ROOT_PATH, type: 'directory' });
      setMode('stage');
      setAppState((prev) => ({ ...prev, error: null }));
      return;
    }

    if (nextMode === 'ledger') {
      setMode('ledger');
      setLedgerSelection(null);
      setLoadingState(true, null);
      void resolveLedgerHopIndexes(
        appState.endpoint, getInitialLedgerHops(appState.rootIndex), false,
      ).then(async (nextHops) => {
        const nextRootIndex = Number.parseInt(nextHops[0].snapshot, 10);
        const nextSelection = await accessibleRemoteSelection(
          new JournalService(appState.endpoint), null, [nextRootIndex, '*state*'],
        );
        if (transition !== navigationTransitionRef.current) return;
        setLedgerHops(nextHops);
        setLedgerSelection(nextSelection);
        setAppState((prev) => ({
          ...prev, rootIndex: nextRootIndex, isLoading: false, error: null,
        }));
      }).catch((error) => {
        if (transition === navigationTransitionRef.current) {
          setAppState((prev) => ({
            ...prev,
            isLoading: false,
            error: `Synchronization failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
          }));
        }
      });
      return;
    }

    setMode(nextMode);
    setLedgerHops(getInitialLedgerHops(appState.rootIndex));
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

  const handleStagePut = async (name: string, input: ResourcePutInput) => {
    if (!journalService || !putParentPath) throw new Error('Put location is unavailable');
    const createdPath = buildStageChildPath(putParentPath, name);
    const created = await journalService.putResource(createdPath, input);
    if (!created) throw new Error('Create conflict: the target already exists');
    setStageSelection({ path: createdPath, type: 'file' });
    setStageExpandedNodes((prev) => {
      const next = new Set(prev);
      next.add(stagePathToTreeNodeId(putParentPath));
      next.add(stagePathToTreeNodeId(createdPath));
      return next;
    });
    setStageRefreshKey((prev) => prev + 1);
    setAppState((prev) => ({ ...prev, error: null }));
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
    const rootPath: JournalPath = [
      ...buildLedgerRouteBasePath(nextHops.slice(0, 1), rootIndex),
      '*state*',
    ];
    try {
      const nextSelection = await accessibleRemoteSelection(
        requesterJournalService ?? journalService,
        rebaseLedgerSelection(ledgerSelection, nextHops, rootPath[0] as number),
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
      hopIndex === index
        ? { ...hop, snapshot: normalizeSnapshotInput(value, hop.maximum ?? appState.rootIndex) }
        : hop,
    );
    void applyLedgerHops(nextHops);
  };

  const handleLedgerStepSnapshot = (index: number, direction: 'older' | 'newer') => {
    const nextHops = ledgerHops.map((hop, hopIndex) => hopIndex === index
      ? {
          ...hop,
          snapshot: stepSnapshotValue(
            hop.snapshot, hop.maximum ?? appState.rootIndex, direction,
          ),
        }
      : hop);
    void applyLedgerHops(nextHops);
  };

  const handleOpenWorkingPeerPicker = async () => {
    if (!journalService) return;
    const request = ++peerPickerRequestRef.current;
    try {
      const peers = await journalService.getBridges(
        workingRoute.length > 0 ? 'working' : undefined,
      );
      if (request !== peerPickerRequestRef.current) return;
      setWorkingPeerChoices(peers.map((peer) => peer.name));
    } catch (error) {
      if (request !== peerPickerRequestRef.current) return;
      setAppState((prev) => ({
        ...prev,
        error: `Bridge lookup failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
      }));
    }
  };

  const handleChooseWorkingPeer = async (peerName: string) => {
    closeWorkingPeerPicker();
    const transition = ++navigationTransitionRef.current;
    const nextRoute = [...workingRoute, peerName];
    if (!journalService || !requesterJournalService) return;
    let nextHops = buildHopsForWorkingRoute(ledgerHops, nextRoute);
    let nextRootIndex = appState.rootIndex;
    let roots: { stage: ExplorerSelection; ledger: ExplorerSelection };
    try {
      nextHops = await resolveLedgerHopIndexes(appState.endpoint, nextHops, false);
      nextRootIndex = Number.parseInt(
        nextHops[0]?.snapshot ?? String(appState.rootIndex), 10,
      );
      roots = await terminalRootSelections(
        new JournalService(appState.endpoint), requesterJournalService,
        nextRoute, nextHops, nextRootIndex,
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
    setAppState((prev) => ({
      ...prev,
      rootIndex: nextHops[0]?.maximum ?? nextRootIndex,
      error: null,
    }));
  };

  const handleRemoveWorkingHop = () => {
    void navigateWorkingRoute(workingRoute.slice(0, -1));
  };

  const handleSelectWorkingHop = (index: number) => {
    void navigateWorkingRoute(workingRoute.slice(0, index));
  };

  const ledgerPinPath = useMemo<JournalPath | null>(() => {
    if (mode !== 'ledger' || !ledgerSelection || ledgerSelection.type === 'directory') return null;
    if (ledgerHops.length <= 1) return ledgerSelection.path;
    return [
      ...buildLedgerRouteBasePath(ledgerHops, appState.rootIndex),
      ...ledgerSelection.path.slice(1),
    ];
  }, [mode, ledgerSelection, ledgerHops, appState.rootIndex]);

  useEffect(() => {
    setLedgerView('content');
  }, [ledgerSelection]);

  useEffect(() => {
    if (sessionStatus !== 'ready' || isApplyingHashRef.current
        || (mode === 'ledger' && appState.rootIndex < 0)) {
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
  }, [sessionStatus, mode, stageSelection, ledgerSelection, ledgerRootPath, ledgerHops, appState.rootIndex]);

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
        localOnlyDisabled={false}
        theme={theme}
        onModeChange={handleModeChange}
        onThemeToggle={() => setTheme((prev) => (prev === 'light' ? 'dark' : 'light'))}
        onSignOut={closeWorkingPeerPicker}
      />

      {mode === 'stage' && (
        <WorkingRouteBar
          route={workingRoute}
          peerChoices={workingPeerChoices}
          onRemoveHop={handleRemoveWorkingHop}
          onSelectHop={handleSelectWorkingHop}
          onOpenPeerPicker={handleOpenWorkingPeerPicker}
          onClosePeerPicker={closeWorkingPeerPicker}
          onChoosePeer={handleChooseWorkingPeer}
        />
      )}

      {mode === 'ledger' && (
        <LedgerRouteBar
          hops={ledgerHops}
          peerChoices={workingPeerChoices}
          rootIndex={appState.rootIndex}
          onSynchronize={synchronizeLedger}
          isSynchronizing={appState.isLoading}
          onSnapshotChange={handleLedgerSnapshotChange}
          onStepSnapshot={handleLedgerStepSnapshot}
          onRemoveHop={handleRemoveWorkingHop}
          onSelectHop={handleSelectWorkingHop}
          onOpenPeerPicker={handleOpenWorkingPeerPicker}
          onClosePeerPicker={closeWorkingPeerPicker}
          onChoosePeer={handleChooseWorkingPeer}
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
            retainedRootPath={retainedRootPath}
            selected={mode === 'stage' ? stageSelection : ledgerSelection}
            expandedNodes={mode === 'stage' ? stageExpandedNodes : ledgerExpandedNodes}
            journalService={journalService}
            requesterJournalService={requesterJournalService}
            currentUser={sessionName}
            refreshKey={mode === 'stage'
              ? stageRefreshKey + stageTreeRefreshKey
              : ledgerRefreshKey}
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
            journalService={contentJournalService}
            refreshKey={mode === 'stage' ? stageRefreshKey : ledgerRefreshKey}
            ledgerView={ledgerView}
            stageReadOnly={mode === 'stage' && isStageNamespaceRoot(stageSelection)}
            readPath={mode === 'ledger' ? ledgerReadPath : null}
            pinJournalService={requesterJournalService}
            pinPath={ledgerPinPath}
            onLedgerViewToggle={() =>
              setLedgerView((prev) => (prev === 'content' ? 'proof' : 'content'))
            }
            onStageCreateFile={async (path) => setPutParentPath(path)}
            onStageCreateDirectory={handleStageCreateDirectory}
            onStageUploadFile={handleStageUploadFile}
            onStageRename={handleRenameStageNode}
            onStageDelete={handleDeleteStageNode}
            onSelectPath={handleContentSelection}
          />
        </div>
      </div>
      )}

      <PutResourceModal
        open={putParentPath !== null}
        onClose={() => setPutParentPath(null)}
        onSubmit={handleStagePut}
      />
    </div>
  );
};

export default App;
